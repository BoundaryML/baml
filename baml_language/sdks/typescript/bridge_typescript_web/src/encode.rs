//! JavaScript values encoded against the types the program declares, on the
//! Web.
//!
//! The rules are JavaScript's and live in `bridge_encode::js`; this module
//! reads `JsValue`s for them. As on Node, what the runtime owns it keeps: the
//! typemap's class lookup, `$types` lowering, the serialization of handles,
//! media, prompts and type values, and the registration of a user function
//! as a host callable all come from the adapter object the TypeScript runtime
//! registers once at load (`registerHostAdapter`). The program's declared
//! types come from the generated SDK's descriptor (`registerProgramDescriptor`,
//! called by `setTypeMap`).
//!
//! WASM is single-threaded: arguments are encoded before the call's first
//! await, and a callback's result inside the completion call.

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
use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use num_bigint::BigInt;
use prost::Message;
use wasm_bindgen::{JsCast, JsValue, prelude::*};

/// The declared types of the loaded program, from the generated SDK. `None`
/// until a typemap with a descriptor is installed.
static FACTS: RwLock<Option<Arc<Facts>>> = RwLock::new(None);

thread_local! {
    /// The runtime's adapter object.
    static ADAPTER: RefCell<Option<Object>> = const { RefCell::new(None) };
    /// The function type each registered host callable was bound to, by its
    /// host-value key. `sys_wasm` owns the callables themselves.
    static SIGNATURES: RefCell<HashMap<u64, RuntimeTy>> = RefCell::new(HashMap::new());
}

/// Install the program descriptor a generated SDK embeds in its typemap
/// (`setTypeMap` calls this), or clear it with `null`.
#[wasm_bindgen(js_name = registerProgramDescriptor)]
pub fn register_program_descriptor(descriptor: Option<String>) -> Result<(), JsValue> {
    let facts = match descriptor {
        Some(text) => Some(Arc::new(Facts::from_descriptor(
            &ProgramDescriptor::decode_embedded(text.as_bytes()).map_err(|error| {
                crate::errors::setup_error(crate::errors::INVALID_ARGUMENT, error.to_string())
            })?,
        ))),
        None => None,
    };
    *FACTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = facts;
    Ok(())
}

/// Install the runtime's adapter: `classOf(value)`, `typeArgsOf(value)`,
/// `leafOf(value)`, `dispatchFor(fn)` and `describe(value)`.
#[wasm_bindgen(js_name = registerHostAdapter)]
pub fn register_host_adapter(adapter: Object) {
    ADAPTER.with(|cell| *cell.borrow_mut() = Some(adapter));
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
/// request bytes. See the Node shim for the contract; the one difference is
/// that a failed encode raises a JavaScript `Error` with code
/// `invalid_argument`.
pub(crate) fn encode_request(
    args_proto: &[u8],
    kwargs: &JsValue,
    cancel: &JsValue,
    sync: bool,
) -> Result<Vec<u8>, JsValue> {
    if kwargs.is_null_or_undefined() && cancel.is_null_or_undefined() {
        return Ok(args_proto.to_vec());
    }
    let mut call = CallFunctionArgs::decode(args_proto).map_err(|error| {
        crate::errors::setup_error(
            crate::errors::INVALID_ARGUMENT,
            format!("malformed CallFunctionArgs: {error}"),
        )
    })?;
    let facts = facts();
    let cx = Cx::new(sync)?;
    let outcome = (|| -> Result<(), Failure> {
        if !cancel.is_null_or_undefined() {
            let encoded = encode(
                &facts,
                &cx.value(cancel.clone()),
                &RuntimeTy::Unknown,
                vec![PathSegment::Argument("$baml.cancel".into())],
            )?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded.value);
        }
        if kwargs.is_null_or_undefined() {
            return Ok(());
        }
        let arguments = entries_of(kwargs)?
            .into_iter()
            .map(|(name, value)| (name, cx.value(value)))
            .collect::<Vec<_>>();
        let (target, signature) = target_signature(&facts, call.call_target.as_ref());
        let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)
            .map_err(|error| Failure::Js(JsValue::from_str(&error.to_string())))?
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

/// Complete an in-flight host call with its JavaScript result, encoded
/// against the return type of the slot the dispatched callable was bound to
/// (the runtime's own I/O callables have none, and send their results as the
/// types they have). A value that does not fit completes the call with a
/// `baml.panics.HostContractViolation` instead.
#[wasm_bindgen(js_name = completeWebHostCallValue)]
pub fn complete_web_host_call_value(call_id: u32, value: JsValue) -> Result<bool, JsValue> {
    let key = sys_wasm::host_callable_key_for_call(call_id);
    let ret = SIGNATURES.with(
        |cell| match key.and_then(|key| cell.borrow().get(&key).cloned()) {
            Some(RuntimeTy::Function { ret, .. }) => *ret,
            Some(_) | None => RuntimeTy::Unknown,
        },
    );
    Ok(
        match encode_value(value, &ret, PathSegment::Result, "host callable") {
            Ok(bytes) => sys_wasm::complete_host_call(call_id, 0, &bytes),
            // The host broke the callable's contract: a panic, as the engine's
            // own return check reports it, not a catchable error.
            Err(error) => sys_wasm::complete_host_call(
                call_id,
                1,
                &bridge_encode::host_contract_violation(&error_message(&error), "web")
                    .encode_to_vec(),
            ),
        },
    )
}

/// The message of a JavaScript error, or the string it is.
fn error_message(error: &JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            error
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| "JavaScript error".to_string())
}

/// Complete an in-flight host call with a thrown BAML value, encoded as the
/// type it has: the engine's throw channel takes it with no declared slot.
#[wasm_bindgen(js_name = completeWebHostCallThrow)]
pub fn complete_web_host_call_throw(call_id: u32, value: JsValue) -> Result<bool, JsValue> {
    let bytes = encode_value(
        value,
        &RuntimeTy::Unknown,
        PathSegment::Result,
        "host callable",
    )?;
    Ok(sys_wasm::complete_host_call(call_id, 1, &bytes))
}

/// Encode one JavaScript value as the bridge would send it where the program
/// declares no type: the value's own type, stated on the wire where a
/// container or class needs it. Returns `InboundValue` bytes; raises for a
/// value the bridge cannot send. For inspection and tests.
#[wasm_bindgen(js_name = _encodeWebInbound)]
pub fn encode_web_inbound(value: JsValue, name: Option<String>) -> Result<Vec<u8>, JsValue> {
    let name = name.unwrap_or_else(|| "value".to_string());
    encode_value(value, &RuntimeTy::Unknown, PathSegment::Argument(name), "")
}

/// Encode one value against `slot` for a path that starts at `root`;
/// `target` names the call in the error, or is empty for none.
fn encode_value(
    value: JsValue,
    slot: &RuntimeTy,
    root: PathSegment,
    target: &str,
) -> Result<Vec<u8>, JsValue> {
    let facts = facts();
    let cx = Cx::new(false)?;
    let outcome = encode(&facts, &cx.value(value), slot, vec![root])
        .map(|encoded| encoded.value.encode_to_vec())
        .map_err(|error| Failure::Encode(target.to_string(), error));
    cx.finish(outcome)
}

/// Forget the signature of a released host callable.
pub(crate) fn forget_signature(key: u64) {
    SIGNATURES.with(|cell| cell.borrow_mut().remove(&key));
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
                Some(_) | None => None,
            };
            ("callable".into(), signature)
        }
        None => ("call".into(), None),
    }
}

enum Failure {
    Js(JsValue),
    Encode(String, EncodeError),
}

impl From<JsValue> for Failure {
    fn from(error: JsValue) -> Self {
        Failure::Js(error)
    }
}

impl From<EncodeError> for Failure {
    fn from(error: EncodeError) -> Self {
        Failure::Encode("host callable".into(), error)
    }
}

/// `Object.entries(value)` as `(key, value)` pairs.
fn entries_of(value: &JsValue) -> Result<Vec<(String, JsValue)>, JsValue> {
    // `typeof`, not `instanceof Object`: the runtime decodes BAML maps into
    // prototype-free objects, which are objects all the same.
    if !value.is_object() {
        return Err(JsValue::from_str("expected an object"));
    }
    let object: &Object = value.unchecked_ref();
    Ok(Object::entries(object)
        .iter()
        .map(|pair| {
            let pair = Array::from(&pair);
            (pair.get(0).as_string().unwrap_or_default(), pair.get(1))
        })
        .collect())
}

struct Cx {
    adapter: Object,
    sync: bool,
    registered: RefCell<Vec<u64>>,
    cloned: RefCell<Vec<u64>>,
    /// The first JavaScript error raised while answering the walker.
    failure: RefCell<Option<JsValue>>,
}

impl Cx {
    fn new(sync: bool) -> Result<Self, JsValue> {
        let adapter = ADAPTER.with(|cell| cell.borrow().clone()).ok_or_else(|| {
            crate::errors::setup_error(
                crate::errors::CLIENT,
                "the BAML runtime's host adapter is not registered; import the runtime package first",
            )
        })?;
        Ok(Self {
            adapter,
            sync,
            registered: RefCell::new(Vec::new()),
            cloned: RefCell::new(Vec::new()),
            failure: RefCell::new(None),
        })
    }

    fn value(&self, value: JsValue) -> Js<View<'_>> {
        Js(View { value, cx: self })
    }

    fn finish<T>(self, outcome: Result<T, Failure>) -> Result<T, JsValue> {
        match outcome {
            Ok(value) => Ok(value),
            Err(failure) => {
                for key in self.registered.borrow().iter() {
                    sys_wasm::release_host_callable(*key);
                    forget_signature(*key);
                }
                for key in self.cloned.borrow().iter() {
                    let _ = handle_cffi::release_handle(*key);
                }
                if let Some(error) = self.failure.borrow_mut().take() {
                    return Err(error);
                }
                Err(match failure {
                    Failure::Js(error) => error,
                    Failure::Encode(target, error) if target.is_empty() => {
                        crate::errors::setup_error(
                            crate::errors::INVALID_ARGUMENT,
                            error.to_string(),
                        )
                    }
                    Failure::Encode(target, error) => crate::errors::setup_error(
                        crate::errors::INVALID_ARGUMENT,
                        format!("{target}: {error}"),
                    ),
                })
            }
        }
    }

    fn fail(&self, error: JsValue) -> HostError {
        let message = error_message(&error);
        self.failure.borrow_mut().get_or_insert(error);
        HostError(message)
    }

    fn adapter_call(&self, name: &str, value: &JsValue) -> Result<JsValue, JsValue> {
        let method = Reflect::get(&self.adapter, &JsValue::from_str(name))?;
        let method = method
            .dyn_ref::<Function>()
            .ok_or_else(|| JsValue::from_str(&format!("host adapter has no `{name}`")))?;
        method.call1(&self.adapter, value)
    }
}

struct View<'a> {
    value: JsValue,
    cx: &'a Cx,
}

impl View<'_> {
    fn kind_inner(&self) -> Result<JsKind, JsValue> {
        let value = &self.value;
        if value.is_null_or_undefined() {
            return Ok(JsKind::Null);
        }
        if let Some(bool) = value.as_bool() {
            return Ok(JsKind::Bool(bool));
        }
        if let Some(number) = value.as_f64() {
            return Ok(JsKind::Number(number));
        }
        if value.is_bigint() {
            let big = js_sys::BigInt::new(value)?;
            let hex: String = big.to_string(16)?.into();
            return BigInt::parse_bytes(hex.as_bytes(), 16)
                .map(JsKind::BigInt)
                .ok_or_else(|| JsValue::from_str("bigint did not render in base sixteen"));
        }
        if let Some(string) = value.as_string() {
            return Ok(JsKind::String(string));
        }
        if Array::is_array(value) {
            return Ok(JsKind::Array);
        }
        if value.is_instance_of::<Uint8Array>() {
            return Ok(JsKind::Bytes(Uint8Array::new(value).to_vec()));
        }
        if value.is_function() {
            return Ok(JsKind::Function);
        }
        if value.is_object() {
            return Ok(JsKind::Object);
        }
        Ok(JsKind::Other)
    }

    fn leaf_inner(&self) -> Result<Option<Leaf>, JsValue> {
        let found = self.cx.adapter_call("leafOf", &self.value)?;
        if found.is_null_or_undefined() {
            return Ok(None);
        }
        let kind = Reflect::get(&found, &JsValue::from_str("kind"))?
            .as_f64()
            .ok_or_else(|| JsValue::from_str("leaf kind is not a number"))?;
        let bytes = Uint8Array::new(&Reflect::get(&found, &JsValue::from_str("bytes"))?).to_vec();
        let value = InboundValue::decode(bytes.as_slice())
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        if let Some(cloned) = Reflect::get(&found, &JsValue::from_str("cloned"))?.as_string() {
            let key: u64 = cloned
                .parse()
                .map_err(|_| JsValue::from_str("cloned handle key is not a uint64"))?;
            self.cx.cloned.borrow_mut().push(key);
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the adapter's kinds are the small non-negative integers matched below"
        )]
        let kind = match kind as i64 as u32 {
            0 => LeafKind::Handle,
            1 => LeafKind::Media(MediaKind::Image),
            2 => LeafKind::Media(MediaKind::Audio),
            3 => LeafKind::Media(MediaKind::Video),
            4 => LeafKind::Media(MediaKind::Pdf),
            5 => LeafKind::Prompt,
            6 => LeafKind::TypeValue,
            other => {
                return Err(JsValue::from_str(&format!(
                    "unknown leaf kind {other} from the host adapter"
                )));
            }
        };
        Ok(Some(Leaf { value, kind }))
    }

    fn register_inner(&self, signature: Option<&RuntimeTy>) -> Result<InboundValue, JsValue> {
        if self.cx.sync {
            return Err(crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                "host callables are only supported on the async call path; use the async API \
                 instead of synchronously invoking a function that takes a callback",
            ));
        }
        // On the Web the runtime registers the dispatch wrapper itself (it
        // owns the frame decoding and execution bookkeeping) and answers the
        // key.
        let key = self.cx.adapter_call("dispatchFor", &self.value)?;
        let key: u64 = key
            .as_string()
            .and_then(|key| key.parse().ok())
            .ok_or_else(|| JsValue::from_str("dispatchFor did not answer a host-value key"))?;
        if let Some(signature) = signature {
            SIGNATURES.with(|cell| cell.borrow_mut().insert(key, signature.clone()));
        }
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

impl JsView for View<'_> {
    fn kind(&self) -> Result<JsKind, HostError> {
        self.kind_inner().map_err(|error| self.cx.fail(error))
    }

    fn elements(&self) -> Result<Vec<Self>, HostError> {
        Ok(Array::from(&self.value)
            .iter()
            .map(|value| View { value, cx: self.cx })
            .collect())
    }

    fn entries(&self) -> Result<Vec<(String, Self)>, HostError> {
        Ok(entries_of(&self.value)
            .map_err(|error| self.cx.fail(error))?
            .into_iter()
            .map(|(name, value)| (name, View { value, cx: self.cx }))
            .collect())
    }

    fn class_name(&self) -> Result<Option<TypeName>, HostError> {
        let found = self
            .cx
            .adapter_call("classOf", &self.value)
            .map_err(|error| self.cx.fail(error))?;
        Ok(found
            .as_string()
            .filter(|fqn| !fqn.is_empty())
            .map(|fqn| TypeName::from_dotted_path(&fqn)))
    }

    fn type_args(&self) -> Result<Option<Vec<RuntimeTy>>, HostError> {
        let found = self
            .cx
            .adapter_call("typeArgsOf", &self.value)
            .map_err(|error| self.cx.fail(error))?;
        if found.is_null_or_undefined() {
            return Ok(None);
        }
        Array::from(&found)
            .iter()
            .map(|bytes| {
                let ty = BamlTy::decode(Uint8Array::new(&bytes).to_vec().as_slice())
                    .map_err(|error| HostError(error.to_string()))?;
                bridge_ctypes::proto_ty_to_runtime_ty(&ty)
                    .map_err(|error| HostError(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
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
            .adapter_call("describe", &self.value)
            .ok()
            .and_then(|found| found.as_string())
            .unwrap_or_else(|| "value".to_string())
    }

    fn string(&self, value: String) -> Self {
        View {
            value: JsValue::from_str(&value),
            cx: self.cx,
        }
    }
}
