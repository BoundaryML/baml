//! JavaScript values encoded against the types the program declares, on Node.
//!
//! The rules are JavaScript's and live in `bridge_encode::js`; this module
//! reads napi values for them. What the runtime owns, it keeps: the typemap's
//! class lookup, `$types` lowering, the serialization of handles, media,
//! prompts and type values, and the dispatch wrapper around a user function
//! all come from a small adapter object the TypeScript runtime registers once
//! at load (`registerHostAdapter`), and the walker calls back into it. The
//! program's declared types come from the generated SDK's descriptor
//! (`registerProgramDescriptor`, called by `setTypeMap`).
//!
//! Everything here runs on the JavaScript thread: arguments are encoded
//! before a call's future is spawned, and a callback's result is encoded
//! inside the completion call.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{MediaKind, Name, RuntimeTy, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    HANDLE_TABLE,
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, BamlTy, CallFunctionArgs, InboundMapEntry, InboundValue,
        call_function_args::CallTarget, inbound_map_entry::Key, inbound_value::Value,
    },
};
use bridge_encode::{
    EncodeError, Facts, FunctionSig, HostError, Leaf, LeafKind, Param, PathSegment, encode,
    encode_arguments,
    js::{Js, JsKind, JsView},
};
use napi::{
    Env, Status, ValueType,
    bindgen_prelude::{
        Array, BigInt as JsBigInt, Buffer, FromNapiValue, Function, JsObjectValue, JsValue, Object,
        ObjectRef, Uint8Array, Unknown,
    },
};
use napi_derive::napi;
use num_bigint::BigInt;
use prost::Message;

/// The declared types of the loaded program, from the generated SDK. `None`
/// until a typemap with a descriptor is installed.
static FACTS: RwLock<Option<Arc<Facts>>> = RwLock::new(None);

thread_local! {
    /// The runtime's adapter object, held on the JavaScript thread.
    static ADAPTER: RefCell<Option<ObjectRef>> = const { RefCell::new(None) };
}

/// Install the program descriptor a generated SDK embeds in its typemap
/// (`setTypeMap` calls this), or clear it with `null`.
#[napi(js_name = "registerProgramDescriptor")]
pub fn register_program_descriptor(descriptor: Option<String>) -> napi::Result<()> {
    let facts = match descriptor {
        Some(text) => Some(Arc::new(Facts::from_descriptor(
            &ProgramDescriptor::decode_embedded(text.as_bytes())
                .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?,
        ))),
        None => None,
    };
    *FACTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = facts;
    Ok(())
}

/// Install the runtime's adapter: `classOf(value)`, `typeArgsOf(value)`,
/// `leafOf(value)`, `dispatchFor(fn)` and `describe(value)`. Called once at
/// module load by the TypeScript runtime.
#[napi(js_name = "registerHostAdapter")]
pub fn register_host_adapter(adapter: Object<'_>) -> napi::Result<()> {
    let reference = adapter.create_ref()?;
    ADAPTER.with(|cell| *cell.borrow_mut() = Some(reference));
    Ok(())
}

fn facts() -> Arc<Facts> {
    FACTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_default()
}

/// Fill a call's `kwargs` (and cancel token) from JavaScript values encoded
/// against the target's declared parameter types, and return the complete
/// request bytes. `sync` is the synchronous entry point, which cannot take
/// host callables.
///
/// Every owner this creates (a registered callable, a cloned handle) is
/// released again if encoding fails, because the engine will never see the
/// bytes; the identified call allocation is retired too.
pub(crate) fn encode_request<'env>(
    env: &'env Env,
    args_proto: &[u8],
    kwargs: Option<Object<'env>>,
    cancel: Option<Unknown<'env>>,
    sync: bool,
) -> napi::Result<Vec<u8>> {
    if kwargs.is_none() && cancel.is_none() {
        return Ok(args_proto.to_vec());
    }
    let mut call = CallFunctionArgs::decode(args_proto).map_err(|error| {
        napi::Error::new(
            Status::InvalidArg,
            format!("malformed CallFunctionArgs: {error}"),
        )
    })?;
    let facts = facts();
    let cx = Cx::new(env, sync)?;
    let outcome = (|| -> Result<(), Failure> {
        if let Some(cancel) = cancel {
            let encoded = encode(
                &facts,
                &cx.value(cancel),
                &RuntimeTy::Unknown,
                vec![PathSegment::Argument("$baml.cancel".into())],
            )?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded.value);
        }
        let Some(kwargs) = kwargs else {
            return Ok(());
        };
        let arguments = cx
            .entries_of(kwargs.to_unknown())?
            .into_iter()
            .map(|(name, value)| (name, cx.value(value)))
            .collect::<Vec<_>>();
        let (target, signature) = target_signature(&facts, call.call_target.as_ref());
        let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)
            .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?
            .type_args
            .into_iter()
            .map(|(name, ty)| (Name::new(&name), ty))
            .collect::<HashMap<_, _>>();
        call.kwargs = match &signature {
            Some(signature) => encode_arguments(&facts, signature, &bindings, &arguments)
                .map_err(|error| Failure::Encode(target.clone(), error))?,
            // No declared types: each argument is the type its value has.
            None => arguments
                .iter()
                .map(|(name, value)| {
                    let encoded = encode(
                        &facts,
                        value,
                        &RuntimeTy::Unknown,
                        vec![PathSegment::Argument(name.clone())],
                    )
                    .map_err(|error| Failure::Encode(target.clone(), error))?;
                    Ok(InboundMapEntry {
                        key: Some(Key::StringKey(name.clone())),
                        value: Some(encoded.value),
                    })
                })
                .collect::<Result<Vec<_>, Failure>>()?,
        };
        Ok(())
    })();
    let call_id = call.call_id;
    match cx.finish(outcome) {
        Ok(()) => Ok(call.encode_to_vec()),
        Err(error) => {
            bridge_cffi::release_function_call_by_id(call_id);
            Err(error)
        }
    }
}

/// Encode a host callable's result or thrown value against `slot`, as
/// `InboundValue` bytes.
pub(crate) fn encode_value<'env>(
    env: &'env Env,
    value: Unknown<'env>,
    slot: &RuntimeTy,
    root: PathSegment,
) -> napi::Result<Vec<u8>> {
    encode_value_for(env, value, slot, root, "host callable")
}

/// Encode one JavaScript value as the bridge would send it where the program
/// declares no type: the value's own type, stated on the wire where a
/// container or class needs it. Returns `InboundValue` bytes; raises for a
/// value the bridge cannot send. For inspection and tests.
#[napi(js_name = "_encodeInbound")]
pub fn encode_inbound<'env>(
    env: &'env Env,
    value: Unknown<'env>,
    name: Option<String>,
) -> napi::Result<Buffer> {
    let name = name.unwrap_or_else(|| "value".to_string());
    encode_value_for(
        env,
        value,
        &RuntimeTy::Unknown,
        PathSegment::Argument(name),
        "",
    )
    .map(Buffer::from)
}

/// Encode one value against `slot` for a path that starts at `root`;
/// `target` names the call in the error, or is empty for none.
fn encode_value_for<'env>(
    env: &'env Env,
    value: Unknown<'env>,
    slot: &RuntimeTy,
    root: PathSegment,
    target: &str,
) -> napi::Result<Vec<u8>> {
    let facts = facts();
    let cx = Cx::new(env, false)?;
    let outcome = encode(&facts, &cx.value(value), slot, vec![root])
        .map(|encoded| encoded.value.encode_to_vec())
        .map_err(|error| Failure::Encode(target.to_string(), error));
    cx.finish(outcome)
}

/// The signature a call target declares: from the descriptor for a named
/// function, from the live handle for a BAML callable, or none.
fn target_signature(facts: &Facts, target: Option<&CallTarget>) -> (String, Option<FunctionSig>) {
    match target {
        Some(CallTarget::FunctionName(name)) => (name.clone(), facts.function(name).cloned()),
        Some(CallTarget::FunctionHandle(key)) => {
            let entry = HANDLE_TABLE.resolve(*key);
            let signature = match entry.as_deref() {
                Some(bridge_ctypes::CffiHandleTableEntry::Adt(
                    bex_project::BexExternalAdt::TaggedHeapHandle {
                        ty:
                            RuntimeTy::Function {
                                params,
                                ret,
                                throws,
                            },
                        ..
                    },
                )) => Some(FunctionSig {
                    class_generic_params: Vec::new(),
                    generic_params: Vec::new(),
                    params: params
                        .iter()
                        .enumerate()
                        .map(|(index, param)| Param {
                            name: param
                                .name
                                .as_ref()
                                .map_or_else(|| format!("arg{index}"), ToString::to_string),
                            ty: param.ty.clone(),
                            has_default: param.is_optional(),
                        })
                        .collect(),
                    ret: (**ret).clone(),
                    throws: (**throws).clone(),
                }),
                // Not a callable, or no longer live: the request decoder
                // reports that; the arguments go as their own types.
                Some(_) | None => None,
            };
            ("callable".into(), signature)
        }
        None => ("call".into(), None),
    }
}

/// Why an encode stopped. A JavaScript error raised while reading a value is
/// kept as it is; a walker error is reported as an invalid argument.
enum Failure {
    Js(napi::Error),
    Encode(String, EncodeError),
}

impl From<napi::Error> for Failure {
    fn from(error: napi::Error) -> Self {
        Failure::Js(error)
    }
}

impl From<EncodeError> for Failure {
    fn from(error: EncodeError) -> Self {
        Failure::Encode("host callable".into(), error)
    }
}

/// What one encode shares across every value it visits: the adapter, the
/// globals it reaches, and the owners it created and may have to release.
struct Cx<'env> {
    env: &'env Env,
    adapter: Object<'env>,
    /// `Object.entries`, for own enumerable entries in insertion order.
    object_entries: Function<'env, Unknown<'env>, Unknown<'env>>,
    uint8_array: Function<'env, (), ()>,
    sync: bool,
    /// Host callables registered during this encode.
    registered: RefCell<Vec<u64>>,
    /// Handle keys cloned for the wire during this encode.
    cloned: RefCell<Vec<u64>>,
    /// The first JavaScript error raised while answering the walker. The
    /// walker sees a non-fit; the caller raises this instead.
    failure: RefCell<Option<napi::Error>>,
}

impl<'env> Cx<'env> {
    fn new(env: &'env Env, sync: bool) -> napi::Result<Self> {
        let adapter = ADAPTER.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|reference| reference.get_value(env))
                .transpose()
        })?;
        let Some(adapter) = adapter else {
            return Err(napi::Error::new(
                Status::GenericFailure,
                "the BAML runtime's host adapter is not registered; import the runtime package first",
            ));
        };
        let global = env.get_global()?;
        // The `Object` constructor is a function; `Object.entries` hangs off it.
        let object_constructor: Function<'env, (), ()> = global.get_named_property("Object")?;
        Ok(Self {
            env,
            adapter,
            object_entries: object_constructor.get_named_property("entries")?,
            uint8_array: global.get_named_property("Uint8Array")?,
            sync,
            registered: RefCell::new(Vec::new()),
            cloned: RefCell::new(Vec::new()),
            failure: RefCell::new(None),
        })
    }

    fn value<'a>(&'a self, value: Unknown<'env>) -> Js<View<'a, 'env>> {
        Js(View { value, cx: self })
    }

    /// Release what this encode created when it did not reach the engine,
    /// and turn the outcome into JavaScript's.
    fn finish<T>(self, outcome: Result<T, Failure>) -> napi::Result<T> {
        match outcome {
            Ok(value) => Ok(value),
            Err(failure) => {
                for key in self.registered.borrow().iter() {
                    crate::host_value::release_registered(*key);
                }
                for key in self.cloned.borrow().iter() {
                    // Best effort: a key the engine never saw has no other owner.
                    let _ = handle_cffi::release_handle(*key);
                }
                if let Some(error) = self.failure.borrow_mut().take() {
                    return Err(error);
                }
                Err(match failure {
                    Failure::Js(error) => error,
                    Failure::Encode(target, error) if target.is_empty() => {
                        napi::Error::new(Status::InvalidArg, error.to_string())
                    }
                    Failure::Encode(target, error) => {
                        napi::Error::new(Status::InvalidArg, format!("{target}: {error}"))
                    }
                })
            }
        }
    }

    /// Record a JavaScript error raised while answering; the walker is told
    /// the value does not fit, and the error is what the caller gets.
    fn fail(&self, error: napi::Error) -> HostError {
        let message = error.to_string();
        self.failure.borrow_mut().get_or_insert(error);
        HostError(message)
    }

    fn adapter_call(&self, name: &str, value: Unknown<'env>) -> napi::Result<Unknown<'env>> {
        let method: Function<'env, Unknown<'env>, Unknown<'env>> =
            self.adapter.get_named_property(name)?;
        method.apply(self.adapter, value)
    }

    /// `Object.entries(value)` as `(key, value)` pairs.
    fn entries_of(&self, value: Unknown<'env>) -> napi::Result<Vec<(String, Unknown<'env>)>> {
        let pairs = Array::from_unknown(self.object_entries.call(value)?)?;
        let mut entries = Vec::with_capacity(pairs.len() as usize);
        for index in 0..pairs.len() {
            let pair = pairs
                .get::<Array>(index)?
                .ok_or_else(|| napi::Error::new(Status::GenericFailure, "Object.entries hole"))?;
            let key = pair
                .get::<String>(0)?
                .ok_or_else(|| napi::Error::new(Status::GenericFailure, "entry without a key"))?;
            let value = match pair.get::<Unknown>(1)? {
                Some(value) => value,
                None => undefined(self.env)?,
            };
            entries.push((key, value));
        }
        Ok(entries)
    }
}

/// One JavaScript value seen through the adapter.
struct View<'a, 'env> {
    value: Unknown<'env>,
    cx: &'a Cx<'env>,
}

impl<'env> View<'_, 'env> {
    fn kind_inner(&self) -> napi::Result<JsKind> {
        Ok(match self.value.get_type()? {
            ValueType::Null | ValueType::Undefined => JsKind::Null,
            ValueType::Boolean => JsKind::Bool(bool::from_unknown(self.value)?),
            ValueType::Number => JsKind::Number(f64::from_unknown(self.value)?),
            ValueType::BigInt => JsKind::BigInt(big_int(JsBigInt::from_unknown(self.value)?)),
            ValueType::String => JsKind::String(String::from_unknown(self.value)?),
            ValueType::Function => JsKind::Function,
            ValueType::Object => {
                if self.value.is_array()? {
                    JsKind::Array
                } else if self.value.instanceof(self.cx.uint8_array)? {
                    JsKind::Bytes(Uint8Array::from_unknown(self.value)?.to_vec())
                } else {
                    JsKind::Object
                }
            }
            ValueType::Symbol | ValueType::External | ValueType::Unknown => JsKind::Other,
        })
    }

    fn leaf_inner(&self) -> napi::Result<Option<Leaf>> {
        let found = self.cx.adapter_call("leafOf", self.value)?;
        if matches!(found.get_type()?, ValueType::Undefined | ValueType::Null) {
            return Ok(None);
        }
        let found = Object::from_unknown(found)?;
        let kind: u32 = found.get_named_property("kind")?;
        let bytes: Uint8Array = found.get_named_property("bytes")?;
        let value = InboundValue::decode(bytes.as_ref())
            .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?;
        if let Some(cloned) = found.get::<String>("cloned")? {
            let key: u64 = cloned.parse().map_err(|_| {
                napi::Error::new(Status::InvalidArg, "cloned handle key is not a uint64")
            })?;
            self.cx.cloned.borrow_mut().push(key);
        }
        let kind = match kind {
            0 => LeafKind::Handle,
            1 => LeafKind::Media(MediaKind::Image),
            2 => LeafKind::Media(MediaKind::Audio),
            3 => LeafKind::Media(MediaKind::Video),
            4 => LeafKind::Media(MediaKind::Pdf),
            5 => LeafKind::Prompt,
            6 => LeafKind::TypeValue,
            other => {
                return Err(napi::Error::new(
                    Status::InvalidArg,
                    format!("unknown leaf kind {other} from the host adapter"),
                ));
            }
        };
        Ok(Some(Leaf { value, kind }))
    }

    fn register_inner(&self, signature: Option<&RuntimeTy>) -> napi::Result<InboundValue> {
        if self.cx.sync {
            return Err(napi::Error::new(
                Status::InvalidArg,
                "host callables are only supported on the async call path; use the async API \
                 instead of synchronously invoking a function that takes a callback",
            ));
        }
        let dispatch = Array::from_unknown(self.cx.adapter_call("dispatchFor", self.value)?)?;
        let dispatch_fn: Function<'env, crate::host_value::DispatchArgs, ()> =
            dispatch.get(0)?.ok_or_else(|| {
                napi::Error::new(Status::GenericFailure, "dispatchFor gave no function")
            })?;
        let marker: Option<&crate::handle::BamlHandle> = match dispatch.get::<Unknown>(1)? {
            Some(marker)
                if !matches!(marker.get_type()?, ValueType::Null | ValueType::Undefined) =>
            {
                Some(<&crate::handle::BamlHandle>::from_unknown(marker)?)
            }
            Some(_) | None => None,
        };
        let key = crate::host_value::register_callable(
            dispatch_fn,
            marker.map(crate::handle::BamlHandle::key_u64),
            signature.cloned(),
        )?;
        self.cx.registered.borrow_mut().push(key);
        Ok(InboundValue {
            value_type: None,
            value: Some(Value::Handle(BamlHandle {
                key,
                handle_type: BamlHandleType::HostValueCallable as i32,
            })),
        })
    }
}

/// The `undefined` value.
fn undefined(env: &Env) -> napi::Result<Unknown<'_>> {
    // SAFETY: `napi_get_undefined` through `Undefined`'s conversion yields a
    // valid value handle for `env`, and the returned `Unknown` borrows `env`.
    unsafe {
        let raw = <napi::bindgen_prelude::Undefined as napi::bindgen_prelude::ToNapiValue>::to_napi_value(
            env.raw(),
            (),
        )?;
        Unknown::from_napi_value(env.raw(), raw)
    }
}

/// A napi `BigInt` (sign and little-endian 64-bit words) as a number.
fn big_int(value: JsBigInt) -> BigInt {
    let mut digits = Vec::with_capacity(value.words.len() * 2);
    for word in &value.words {
        digits.push((*word & u64::from(u32::MAX)) as u32);
        digits.push((*word >> 32) as u32);
    }
    let magnitude = num_bigint::BigUint::new(digits);
    if value.sign_bit {
        -BigInt::from(magnitude)
    } else {
        BigInt::from(magnitude)
    }
}

impl<'env> JsView for View<'_, 'env> {
    fn kind(&self) -> Result<JsKind, HostError> {
        self.kind_inner().map_err(|error| self.cx.fail(error))
    }

    fn elements(&self) -> Result<Vec<Self>, HostError> {
        let array = Array::from_unknown(self.value).map_err(|error| self.cx.fail(error))?;
        let mut items = Vec::with_capacity(array.len() as usize);
        for index in 0..array.len() {
            // A hole in a sparse array reads as `undefined`.
            let item = match array.get::<Unknown>(index) {
                Ok(Some(item)) => item,
                Ok(None) => undefined(self.cx.env).map_err(|error| self.cx.fail(error))?,
                Err(error) => return Err(self.cx.fail(error)),
            };
            items.push(View {
                value: item,
                cx: self.cx,
            });
        }
        Ok(items)
    }

    fn entries(&self) -> Result<Vec<(String, Self)>, HostError> {
        Ok(self
            .cx
            .entries_of(self.value)
            .map_err(|error| self.cx.fail(error))?
            .into_iter()
            .map(|(name, value)| (name, View { value, cx: self.cx }))
            .collect())
    }

    fn class_name(&self) -> Result<Option<TypeName>, HostError> {
        let found = self
            .cx
            .adapter_call("classOf", self.value)
            .map_err(|error| self.cx.fail(error))?;
        match found.get_type().map_err(|error| self.cx.fail(error))? {
            ValueType::String => {
                let fqn = String::from_unknown(found).map_err(|error| self.cx.fail(error))?;
                Ok((!fqn.is_empty()).then(|| TypeName::from_dotted_path(&fqn)))
            }
            _ => Ok(None),
        }
    }

    fn type_args(&self) -> Result<Option<Vec<RuntimeTy>>, HostError> {
        let found = self
            .cx
            .adapter_call("typeArgsOf", self.value)
            .map_err(|error| self.cx.fail(error))?;
        if matches!(
            found.get_type().map_err(|error| self.cx.fail(error))?,
            ValueType::Undefined | ValueType::Null
        ) {
            return Ok(None);
        }
        let encoded = Array::from_unknown(found).map_err(|error| self.cx.fail(error))?;
        let mut args = Vec::with_capacity(encoded.len() as usize);
        for index in 0..encoded.len() {
            let bytes = encoded
                .get::<Uint8Array>(index)
                .map_err(|error| self.cx.fail(error))?
                .ok_or_else(|| HostError("typeArgsOf gave a hole".into()))?;
            let ty =
                BamlTy::decode(bytes.as_ref()).map_err(|error| HostError(error.to_string()))?;
            args.push(
                bridge_ctypes::proto_ty_to_runtime_ty(&ty)
                    .map_err(|error| HostError(error.to_string()))?,
            );
        }
        Ok(Some(args))
    }

    fn leaf(&self) -> Result<Option<Leaf>, HostError> {
        self.leaf_inner().map_err(|error| self.cx.fail(error))
    }

    fn register_callable(&self, signature: Option<&RuntimeTy>) -> Result<InboundValue, HostError> {
        self.register_inner(signature)
            .map_err(|error| self.cx.fail(error))
    }

    fn describe(&self) -> String {
        self.cx
            .adapter_call("describe", self.value)
            .and_then(String::from_unknown)
            .unwrap_or_else(|_| "value".to_string())
    }

    fn string(&self, value: String) -> Self {
        let value = self
            .cx
            .env
            .create_string(&value)
            .map(|string| string.to_unknown())
            .unwrap_or_else(|_| self.value);
        View { value, cx: self.cx }
    }
}
