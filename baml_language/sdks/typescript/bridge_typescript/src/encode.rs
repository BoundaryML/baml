//! JavaScript values encoded against the types the program declares, on Node.
//!
//! The rules are JavaScript's and live in `bridge_encode::js`; this module
//! reads napi values for them. What the runtime owns, it keeps: the typemap's
//! class lookup, `$types` lowering, the serialization of handles, media,
//! prompts and type values, and the dispatch wrapper around a user function
//! all come from a small adapter object the TypeScript runtime registers once
//! at load (`registerHostAdapter`), and the bridge calls back into it. The
//! program's declared types come from the generated SDK's descriptor
//! (`registerProgramDescriptor`, called by `setTypeMap`).
//!
//! Everything here runs on the JavaScript thread: arguments are encoded
//! before a call's future is spawned, and a callback's result is encoded
//! inside the completion call.

use std::{
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{MediaKind, Name, RuntimeTy, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    HANDLE_TABLE,
    baml_bridge::cffi::{BamlTy, CallFunctionArgs, InboundValue, call_function_args::CallTarget},
};
use bridge_encode::{
    Completion, Error, Facts, FunctionSig, Host, LeafKind, LeafWire, Owner, Owners, Param, Root,
    encode, encode_arguments,
    js::{Js, JsKind, JsView},
};
use napi::{
    Env, JsString, Status, ValueType,
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
        Some(text) => {
            let descriptor = ProgramDescriptor::decode_embedded(text.as_bytes())
                .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?;
            Some(Arc::new(Facts::from_descriptor(&descriptor).map_err(
                |error| napi::Error::new(Status::InvalidArg, error.to_string()),
            )?))
        }
        None => None,
    };
    *FACTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = facts;
    Ok(())
}

/// Install the runtime's adapter: `classOf(value)`, `typeArgsOf(value)`,
/// `leafKindOf(value)`, `leafOf(value)`, `dispatchFor(fn)` and
/// `describe(value)`. Called once at module load by the TypeScript runtime.
#[napi(js_name = "registerHostAdapter")]
pub fn register_host_adapter(env: Env, adapter: Object<'_>) -> napi::Result<()> {
    let reference = adapter.create_ref()?;
    let previous = ADAPTER.with(|cell| cell.borrow_mut().replace(reference));
    if let Some(previous) = previous {
        previous.unref(&env)?;
    }
    // A reference outlives the environment only as a leak napi reports; it
    // is let go when the environment shuts down.
    env.add_env_cleanup_hook(env, |env| {
        if let Some(reference) = ADAPTER.with(|cell| cell.borrow_mut().take()) {
            // The environment is going away: nothing to do about a failure.
            let _ = reference.unref(&env);
        }
    })?;
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
    let mut owners = Owners::new(&cx);
    let (target, signature) = target_signature(&facts, call.call_target.as_ref());
    let outcome = (|| -> napi::Result<()> {
        if let Some(cancel) = cancel {
            let encoded = encode(
                &facts,
                cx.value(cancel),
                &RuntimeTy::Unknown,
                Root::Argument("$baml.cancel".into()),
                &mut owners,
            )
            .map_err(|error| raise(&target, error))?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded);
        }
        let Some(kwargs) = kwargs else {
            return Ok(());
        };
        let arguments = cx
            .entries_of(kwargs.to_unknown())?
            .into_iter()
            .map(|(name, value)| (name, cx.value(value)))
            .collect::<Vec<_>>();
        let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)
            .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?
            .type_args
            .into_iter()
            .map(|(name, ty)| (Name::new(&name), ty))
            .collect::<HashMap<_, _>>();
        call.kwargs = encode_arguments(&facts, &signature, &bindings, arguments, &mut owners)
            .map_err(|error| raise(&target, error))?;
        Ok(())
    })();
    match outcome {
        Ok(()) => {
            owners.keep();
            Ok(call.encode_to_vec())
        }
        Err(error) => {
            drop(owners);
            bridge_cffi::release_function_call_by_id(call.call_id);
            Err(error)
        }
    }
}

/// Encode a host callable's result or thrown value against `slot`. What the
/// result means for the call is the caller's to decide from the
/// [`Completion`].
pub(crate) fn encode_result<'env>(
    env: &'env Env,
    value: Unknown<'env>,
    slot: &RuntimeTy,
) -> Completion<napi::Error> {
    let facts = facts();
    let cx = match Cx::new(env, false) {
        Ok(cx) => cx,
        Err(error) => return Completion::HostError(error),
    };
    let mut owners = Owners::new(&cx);
    let result = encode(&facts, cx.value(value), slot, Root::Result, &mut owners);
    if result.is_ok() {
        owners.keep();
    }
    Completion::from(result)
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
    let facts = facts();
    let cx = Cx::new(env, false)?;
    let mut owners = Owners::new(&cx);
    let encoded = encode(
        &facts,
        cx.value(value),
        &RuntimeTy::Unknown,
        Root::Argument(name.unwrap_or_else(|| "value".to_string())),
        &mut owners,
    )
    .map_err(|error| raise("", error))?;
    // The bytes carry what the encode created (a cloned handle's key), as
    // they would to the engine; the caller owns them and releases them.
    owners.keep();
    Ok(Buffer::from(encoded.encode_to_vec()))
}

/// An encode's failure as JavaScript raises it: a JavaScript error as it is,
/// a refusal as an invalid argument naming `target` (the call), or the
/// refusal alone when `target` is empty.
fn raise(target: &str, error: Error<napi::Error>) -> napi::Error {
    match error {
        Error::Host(error) | Error::Commit(error) => error,
        Error::Value(error) if target.is_empty() => {
            napi::Error::new(Status::InvalidArg, error.to_string())
        }
        Error::Value(error) => napi::Error::new(Status::InvalidArg, format!("{target}: {error}")),
    }
}

/// The signature a call target declares: from the descriptor for a named
/// function, from the live handle for a BAML callable, or none, in which
/// case every argument goes as the type its value has.
fn target_signature(facts: &Facts, target: Option<&CallTarget>) -> (String, FunctionSig) {
    let untyped = || FunctionSig {
        class_generic_params: Vec::new(),
        generic_params: Vec::new(),
        params: Vec::new(),
        ret: RuntimeTy::Unknown,
        throws: RuntimeTy::Unknown,
    };
    match target {
        Some(CallTarget::FunctionName(name)) => (
            name.clone(),
            facts.function(name).cloned().unwrap_or_else(untyped),
        ),
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
                )) => FunctionSig {
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
                },
                // Not a callable, or no longer live: the request decoder
                // reports that; the arguments go as their own types.
                Some(_) | None => untyped(),
            };
            ("callable".into(), signature)
        }
        None => ("call".into(), untyped()),
    }
}

/// What one encode shares across every value it visits: the adapter and the
/// globals it reaches.
struct Cx<'env> {
    env: &'env Env,
    adapter: Object<'env>,
    /// `Object.entries`, for own enumerable entries in insertion order.
    object_entries: Function<'env, Unknown<'env>, Unknown<'env>>,
    /// `Object.getPrototypeOf`, to tell a plain object from an instance.
    get_prototype_of: Function<'env, Unknown<'env>, Unknown<'env>>,
    object_prototype: Unknown<'env>,
    /// `Array.isArray`: true for a Proxy of an array too, as on the Web.
    array_is_array: Function<'env, Unknown<'env>, bool>,
    uint8_array: Function<'env, (), ()>,
    sync: bool,
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
        // The `Object` constructor is a function; its statics hang off it.
        let object_constructor: Function<'env, (), ()> = global.get_named_property("Object")?;
        let array_constructor: Function<'env, (), ()> = global.get_named_property("Array")?;
        Ok(Self {
            env,
            adapter,
            object_entries: object_constructor.get_named_property("entries")?,
            get_prototype_of: object_constructor.get_named_property("getPrototypeOf")?,
            object_prototype: object_constructor.get_named_property("prototype")?,
            array_is_array: array_constructor.get_named_property("isArray")?,
            uint8_array: global.get_named_property("Uint8Array")?,
            sync,
        })
    }

    fn value<'a>(&'a self, value: Unknown<'env>) -> Js<View<'a, 'env>> {
        Js(View { value, cx: self })
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
                .get::<Unknown>(0)?
                .ok_or_else(|| napi::Error::new(Status::GenericFailure, "entry without a key"))?;
            let key = well_formed(key)?;
            let value = match pair.get::<Unknown>(1)? {
                Some(value) => value,
                None => undefined(self.env)?,
            };
            entries.push((key, value));
        }
        Ok(entries)
    }

    /// Is `value` a plain object: prototype `Object.prototype` or none?
    fn is_plain_object(&self, value: Unknown<'env>) -> napi::Result<bool> {
        let prototype = self.get_prototype_of.call(value)?;
        Ok(matches!(prototype.get_type()?, ValueType::Null)
            || self.env.strict_equals(prototype, self.object_prototype)?)
    }

    /// The leaf kind the adapter answers for `value`, if it is one.
    fn leaf_kind_of(&self, value: Unknown<'env>) -> napi::Result<Option<LeafKind>> {
        let found = self.adapter_call("leafKindOf", value)?;
        if matches!(found.get_type()?, ValueType::Undefined | ValueType::Null) {
            return Ok(None);
        }
        leaf_kind(u32::from_unknown(found)?).map(Some)
    }
}

/// The leaf kind the adapter numbers `kind`.
fn leaf_kind(kind: u32) -> napi::Result<LeafKind> {
    Ok(match kind {
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
    })
}

/// The leaf payload the adapter serialized, as the wire variant of `kind`.
fn leaf_wire(kind: LeafKind, value: InboundValue) -> napi::Result<LeafWire> {
    use bridge_ctypes::baml_bridge::cffi::inbound_value::Value;
    match (kind, value.value) {
        (LeafKind::Handle, Some(Value::Handle(handle))) => Ok(LeafWire::Handle(handle)),
        (LeafKind::Media(_), Some(Value::MediaValue(media))) => Ok(LeafWire::Media(media)),
        (LeafKind::Prompt, Some(Value::PromptAstValue(prompt))) => Ok(LeafWire::Prompt(prompt)),
        (LeafKind::TypeValue, Some(Value::TyDefValue(definition))) => {
            Ok(LeafWire::TypeValue(definition))
        }
        (LeafKind::Handle | LeafKind::Media(_) | LeafKind::Prompt | LeafKind::TypeValue, _) => {
            Err(napi::Error::new(
                Status::GenericFailure,
                format!("the host adapter serialized a {kind:?} leaf as another kind"),
            ))
        }
    }
}

impl<'env> Host<Js<View<'_, 'env>>> for Cx<'env> {
    fn register(&self, callable: &Js<View<'_, 'env>>, signature: &RuntimeTy) -> napi::Result<u64> {
        if self.sync {
            return Err(napi::Error::new(
                Status::InvalidArg,
                "host callables are only supported on the async call path; use the async API \
                 instead of synchronously invoking a function that takes a callback",
            ));
        }
        let dispatch = Array::from_unknown(self.adapter_call("dispatchFor", callable.0.value)?)?;
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
        crate::host_value::register_callable(
            dispatch_fn,
            marker.map(crate::handle::BamlHandle::key_u64),
            signature.clone(),
        )
    }

    fn leaf(
        &self,
        leaf: &Js<View<'_, 'env>>,
        kind: LeafKind,
    ) -> napi::Result<(LeafWire, Option<Owner>)> {
        let found = Object::from_unknown(self.adapter_call("leafOf", leaf.0.value)?)?;
        let bytes: Uint8Array = found.get_named_property("bytes")?;
        let value = InboundValue::decode(bytes.as_ref())
            .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?;
        let owner = match found.get::<String>("cloned")? {
            Some(cloned) => Some(Owner::Handle(cloned.parse().map_err(|_| {
                napi::Error::new(Status::InvalidArg, "cloned handle key is not a uint64")
            })?)),
            None => None,
        };
        Ok((leaf_wire(kind, value)?, owner))
    }

    fn release(&self, owner: Owner) {
        match owner {
            Owner::Callable(key) => crate::host_value::release_registered(key),
            // Best effort: a key the engine never saw has no other owner.
            Owner::Handle(key) => {
                let _ = handle_cffi::release_handle(key);
            }
        }
    }
}

/// One JavaScript value seen through the adapter.
struct View<'a, 'env> {
    value: Unknown<'env>,
    cx: &'a Cx<'env>,
}

impl<'env> View<'_, 'env> {
    /// What an object is: a leaf the runtime owns, an instance of a class the
    /// typemap knows, a plain object, or something else.
    fn object_kind(&self) -> napi::Result<JsKind> {
        if let Some(kind) = self.cx.leaf_kind_of(self.value)? {
            return Ok(JsKind::Leaf(kind));
        }
        let class = self.cx.adapter_call("classOf", self.value)?;
        if matches!(class.get_type()?, ValueType::String) {
            let fqn = String::from_unknown(class)?;
            if !fqn.is_empty() {
                return Ok(JsKind::Instance(TypeName::from_dotted_path(&fqn)));
            }
        }
        if self.cx.is_plain_object(self.value)? {
            return Ok(JsKind::PlainObject);
        }
        Ok(JsKind::Other)
    }
}

/// The text of a JavaScript string, refused when it is not well formed: a
/// lone surrogate has no UTF-8 spelling and is never replaced silently.
fn well_formed(value: Unknown<'_>) -> napi::Result<String> {
    // `as_str` drops the terminator napi writes and refuses a lone surrogate.
    JsString::from_unknown(value)?
        .into_utf16()?
        .as_str()
        .map_err(|_| {
            napi::Error::new(
                Status::InvalidArg,
                "string is not well formed: it contains a lone surrogate",
            )
        })
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
    type Error = napi::Error;

    fn kind(&self) -> napi::Result<JsKind> {
        Ok(match self.value.get_type()? {
            ValueType::Null | ValueType::Undefined => JsKind::Null,
            ValueType::Boolean => JsKind::Bool(bool::from_unknown(self.value)?),
            ValueType::Number => JsKind::Number(f64::from_unknown(self.value)?),
            ValueType::BigInt => JsKind::BigInt(big_int(JsBigInt::from_unknown(self.value)?)),
            ValueType::String => JsKind::String,
            // A function the runtime knows as a BAML closure crosses as its
            // handle; any other is a host callable.
            ValueType::Function => match self.cx.leaf_kind_of(self.value)? {
                Some(kind) => JsKind::Leaf(kind),
                None => JsKind::Function,
            },
            ValueType::Object => {
                if self.cx.array_is_array.call(self.value)? {
                    JsKind::Array
                } else if self.value.instanceof(self.cx.uint8_array)? {
                    JsKind::Bytes
                } else {
                    self.object_kind()?
                }
            }
            ValueType::Symbol | ValueType::External | ValueType::Unknown => JsKind::Other,
        })
    }

    fn elements(&self) -> napi::Result<Vec<Self>> {
        // Through the object protocol, so a Proxy of an array reads as the
        // array it stands for.
        let array = Object::from_unknown(self.value)?;
        let len: u32 = array.get_named_property("length")?;
        let mut items = Vec::with_capacity(len as usize);
        for index in 0..len {
            // A hole in a sparse array reads as `undefined`.
            let item = match array.get_element::<Option<Unknown>>(index)? {
                Some(item) => item,
                None => undefined(self.cx.env)?,
            };
            items.push(View {
                value: item,
                cx: self.cx,
            });
        }
        Ok(items)
    }

    fn entries(&self) -> napi::Result<Vec<(String, Self)>> {
        Ok(self
            .cx
            .entries_of(self.value)?
            .into_iter()
            .map(|(name, value)| (name, View { value, cx: self.cx }))
            .collect())
    }

    fn type_args(&self) -> napi::Result<Option<Vec<RuntimeTy>>> {
        let found = self.cx.adapter_call("typeArgsOf", self.value)?;
        if matches!(found.get_type()?, ValueType::Undefined | ValueType::Null) {
            return Ok(None);
        }
        let encoded = Array::from_unknown(found)?;
        let mut args = Vec::with_capacity(encoded.len() as usize);
        for index in 0..encoded.len() {
            let bytes = encoded.get::<Uint8Array>(index)?.ok_or_else(|| {
                napi::Error::new(Status::GenericFailure, "typeArgsOf gave a hole")
            })?;
            let ty = BamlTy::decode(bytes.as_ref())
                .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?;
            args.push(
                bridge_ctypes::proto_ty_to_runtime_ty(&ty)
                    .map_err(|error| napi::Error::new(Status::InvalidArg, error.to_string()))?,
            );
        }
        Ok(Some(args))
    }

    fn text(&self) -> napi::Result<Cow<'_, str>> {
        well_formed(self.value).map(Cow::Owned)
    }

    fn bytes(&self) -> napi::Result<Cow<'_, [u8]>> {
        Ok(Cow::Owned(Uint8Array::from_unknown(self.value)?.to_vec()))
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
