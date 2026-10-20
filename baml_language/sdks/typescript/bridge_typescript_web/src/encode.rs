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
    borrow::Cow,
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, RwLock},
};

use baml_sdkgen_types::ProgramDescriptor;
use baml_type::{MediaKind, Name, RuntimeTy, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    HANDLE_TABLE, HostKey, TableKey,
    baml_bridge::cffi::{BamlTy, CallFunctionArgs, InboundValue, call_function_args::CallTarget},
};
use bridge_encode::{
    BridgeFault, Completion, Error, Facts, FunctionSig, Host, LeafKind, LeafWire, Owner, Owners,
    Param, Root, encode, encode_arguments,
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
    static SIGNATURES: RefCell<HashMap<HostKey, RuntimeTy>> = RefCell::new(HashMap::new());
}

/// Install the program descriptor a generated SDK embeds in its typemap
/// (`setTypeMap` calls this), or clear it with `null`.
#[wasm_bindgen(js_name = registerProgramDescriptor)]
pub fn register_program_descriptor(descriptor: Option<String>) -> Result<(), JsValue> {
    let invalid =
        |error: String| crate::errors::setup_error(crate::errors::INVALID_ARGUMENT, error);
    let facts = match descriptor {
        Some(text) => {
            let descriptor = ProgramDescriptor::decode_embedded(text.as_bytes())
                .map_err(|error| invalid(error.to_string()))?;
            Some(Arc::new(
                Facts::from_descriptor(&descriptor).map_err(|error| invalid(error.to_string()))?,
            ))
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
/// `describe(value)`.
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
    let mut owners = Owners::new(&cx);
    let (target, signature) = target_signature(&facts, call.call_target.as_ref());
    let outcome = (|| -> Result<(), JsValue> {
        if !cancel.is_null_or_undefined() {
            let encoded = encode(
                &facts,
                cx.value(cancel.clone()),
                &RuntimeTy::Unknown,
                Root::Argument("$baml.cancel".into()),
                &mut owners,
            )
            .map_err(|error| raise(&target, error))?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded);
        }
        if kwargs.is_null_or_undefined() {
            return Ok(());
        }
        let arguments = entries_of(kwargs)?
            .into_iter()
            .map(|(name, value)| (name, cx.value(value)))
            .collect::<Vec<_>>();
        let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)
            .map_err(|error| JsValue::from_str(&error.to_string()))?
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

/// Complete an in-flight host call with its JavaScript result, encoded
/// against the return type of the slot the dispatched callable was bound to
/// (the runtime's own I/O callables have none, and send their results as the
/// types they have). A value that does not fit completes the call as a
/// `baml.panics.HostContractViolation`; a value the bridge could not carry as
/// a `baml.panics.SdkPanic`; an error the callable's own code raised while
/// the value was read is raised here, and the runtime completes the call with
/// it. Returns the outcome the engine sees, `"ok"` or `"error"`, for the
/// runtime's instrumentation.
#[wasm_bindgen(js_name = completeWebHostCallValue)]
pub fn complete_web_host_call_value(call_id: u32, value: JsValue) -> Result<String, JsValue> {
    let key = sys_wasm::host_callable_key_for_call(call_id);
    let ret = SIGNATURES.with(
        |cell| match key.and_then(|key| cell.borrow().get(&key).cloned()) {
            Some(RuntimeTy::Function { ret, .. }) => *ret,
            Some(_) | None => RuntimeTy::Unknown,
        },
    );
    let outcome = match encode_result(value, &ret) {
        Completion::Wire(value) => {
            sys_wasm::complete_host_call(call_id, 0, &value.encode_to_vec());
            "ok"
        }
        Completion::HostError(error) => return Err(error),
        // The host broke the callable's contract: a panic, as the engine's
        // own return check reports it, not a catchable error.
        Completion::ContractViolation(error) => {
            sys_wasm::complete_host_call(
                call_id,
                1,
                &bridge_encode::host_contract_violation(&error.to_string(), "web").encode_to_vec(),
            );
            "error"
        }
        Completion::BridgeFailure(fault) => {
            let message = match fault {
                BridgeFault::Value(error) => error.to_string(),
                BridgeFault::Host(error) => error_message(&error),
            };
            sys_wasm::complete_host_call(
                call_id,
                1,
                &bridge_encode::sdk_panic(&message).encode_to_vec(),
            );
            "error"
        }
    };
    Ok(outcome.to_string())
}

/// Complete an in-flight host call with a thrown BAML value, encoded as the
/// type it has: the engine's throw channel takes it with no declared slot.
#[wasm_bindgen(js_name = completeWebHostCallThrow)]
pub fn complete_web_host_call_throw(call_id: u32, value: JsValue) -> Result<bool, JsValue> {
    let value = match encode_result(value, &RuntimeTy::Unknown) {
        Completion::Wire(value) => value,
        Completion::HostError(error) => return Err(error),
        Completion::ContractViolation(error)
        | Completion::BridgeFailure(BridgeFault::Value(error)) => {
            return Err(crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                error.to_string(),
            ));
        }
        Completion::BridgeFailure(BridgeFault::Host(error)) => return Err(error),
    };
    Ok(sys_wasm::complete_host_call(
        call_id,
        1,
        &value.encode_to_vec(),
    ))
}

/// Encode one JavaScript value as the bridge would send it where the program
/// declares no type: the value's own type, stated on the wire where a
/// container or class needs it. Returns `InboundValue` bytes; raises for a
/// value the bridge cannot send. For inspection and tests.
#[wasm_bindgen(js_name = _encodeWebInbound)]
pub fn encode_web_inbound(value: JsValue, name: Option<String>) -> Result<Vec<u8>, JsValue> {
    let facts = facts();
    let cx = Cx::new(false)?;
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
    Ok(encoded.encode_to_vec())
}

/// Encode a host callable's result or thrown value against `slot`.
fn encode_result(value: JsValue, slot: &RuntimeTy) -> Completion<JsValue> {
    let facts = facts();
    let cx = match Cx::new(false) {
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

/// An encode's failure as JavaScript raises it: a JavaScript error as it is,
/// a refusal as an `invalid_argument` error naming `target` (the call), or
/// the refusal alone when `target` is empty.
fn raise(target: &str, error: Error<JsValue>) -> JsValue {
    match error {
        Error::Host(error) | Error::Commit(error) => error,
        Error::Value(error) if target.is_empty() => {
            crate::errors::setup_error(crate::errors::INVALID_ARGUMENT, error.to_string())
        }
        Error::Value(error) => crate::errors::setup_error(
            crate::errors::INVALID_ARGUMENT,
            format!("{target}: {error}"),
        ),
    }
}

/// The message of a JavaScript error, or the string it is.
pub(crate) fn error_message(error: &JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            error
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| "JavaScript error".to_string())
}

/// Forget the signature of a released host callable.
pub(crate) fn forget_signature(key: HostKey) {
    SIGNATURES.with(|cell| cell.borrow_mut().remove(&key));
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
            let entry = TableKey::new(*key).and_then(|key| HANDLE_TABLE.resolve(key));
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
                Some(_) | None => untyped(),
            };
            ("callable".into(), signature)
        }
        None => ("call".into(), untyped()),
    }
}

/// The text of a JavaScript string, refused when it is not well formed: a
/// lone surrogate has no UTF-8 spelling and is never replaced silently.
fn well_formed(value: &JsValue) -> Result<String, JsValue> {
    if !value.is_string() {
        return Err(JsValue::from_str("expected a string"));
    }
    let units: Vec<u16> = js_sys::JsString::from(value.clone()).iter().collect();
    String::from_utf16(&units).map_err(|_| {
        crate::errors::setup_error(
            crate::errors::INVALID_ARGUMENT,
            "string is not well formed: it contains a lone surrogate",
        )
    })
}

/// `Object.entries(value)` as `(key, value)` pairs.
fn entries_of(value: &JsValue) -> Result<Vec<(String, JsValue)>, JsValue> {
    // `typeof`, not `instanceof Object`: the runtime decodes BAML maps into
    // prototype-free objects, which are objects all the same.
    if !value.is_object() {
        return Err(JsValue::from_str("expected an object"));
    }
    let object: &Object = value.unchecked_ref();
    Object::entries(object)
        .iter()
        .map(|pair| {
            let pair = Array::from(&pair);
            Ok((well_formed(&pair.get(0))?, pair.get(1)))
        })
        .collect()
}

/// What one encode shares across every value it visits: the adapter, and
/// `Object.prototype` to tell a plain object from an instance.
struct Cx {
    adapter: Object,
    object_prototype: JsValue,
    sync: bool,
}

impl Cx {
    fn new(sync: bool) -> Result<Self, JsValue> {
        let adapter = ADAPTER.with(|cell| cell.borrow().clone()).ok_or_else(|| {
            crate::errors::setup_error(
                crate::errors::CLIENT,
                "the BAML runtime's host adapter is not registered; import the runtime package first",
            )
        })?;
        let object_constructor = Reflect::get(&js_sys::global(), &JsValue::from_str("Object"))?;
        let object_prototype = Reflect::get(&object_constructor, &JsValue::from_str("prototype"))?;
        Ok(Self {
            adapter,
            object_prototype,
            sync,
        })
    }

    fn value(&self, value: JsValue) -> Js<View<'_>> {
        Js(View { value, cx: self })
    }

    fn adapter_call(&self, name: &str, value: &JsValue) -> Result<JsValue, JsValue> {
        let method = Reflect::get(&self.adapter, &JsValue::from_str(name))?;
        let method = method
            .dyn_ref::<Function>()
            .ok_or_else(|| JsValue::from_str(&format!("host adapter has no `{name}`")))?;
        method.call1(&self.adapter, value)
    }

    /// Is `value` a plain object: prototype `Object.prototype` or none?
    fn is_plain_object(&self, value: &JsValue) -> Result<bool, JsValue> {
        let prototype = Reflect::get_prototype_of(value)?;
        Ok(prototype.is_null() || JsValue::from(prototype) == self.object_prototype)
    }

    /// The leaf kind the adapter answers for `value`, if it is one.
    fn leaf_kind_of(&self, value: &JsValue) -> Result<Option<LeafKind>, JsValue> {
        let found = self.adapter_call("leafKindOf", value)?;
        if found.is_null_or_undefined() {
            return Ok(None);
        }
        let kind = found
            .as_f64()
            .ok_or_else(|| JsValue::from_str("leaf kind is not a number"))?;
        leaf_kind(kind).map(Some)
    }
}

/// The leaf kind the adapter numbers `kind`.
fn leaf_kind(kind: f64) -> Result<LeafKind, JsValue> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the adapter's kinds are the small non-negative integers matched below"
    )]
    let kind = kind as i64 as u32;
    Ok(match kind {
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
    })
}

/// The leaf payload the adapter serialized, as the wire variant of `kind`.
fn leaf_wire(kind: LeafKind, value: InboundValue) -> Result<LeafWire, JsValue> {
    use bridge_ctypes::baml_bridge::cffi::inbound_value::Value;
    match (kind, value.value) {
        (LeafKind::Handle, Some(Value::Handle(handle))) => Ok(LeafWire::Handle(handle)),
        (LeafKind::Media(_), Some(Value::MediaValue(media))) => Ok(LeafWire::Media(media)),
        (LeafKind::Prompt, Some(Value::PromptAstValue(prompt))) => Ok(LeafWire::Prompt(prompt)),
        (LeafKind::TypeValue, Some(Value::TyDefValue(definition))) => {
            Ok(LeafWire::TypeValue(definition))
        }
        (LeafKind::Handle | LeafKind::Media(_) | LeafKind::Prompt | LeafKind::TypeValue, _) => {
            Err(JsValue::from_str(&format!(
                "the host adapter serialized a {kind:?} leaf as another kind"
            )))
        }
    }
}

impl Host<Js<View<'_>>> for Cx {
    fn register(&self, callable: &Js<View<'_>>, signature: &RuntimeTy) -> Result<HostKey, JsValue> {
        if self.sync {
            return Err(crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                "host callables are only supported on the async call path; use the async API \
                 instead of synchronously invoking a function that takes a callback",
            ));
        }
        // On the Web the runtime registers the dispatch wrapper itself (it
        // owns the frame decoding and execution bookkeeping) and answers the
        // key.
        let key = self.adapter_call("dispatchFor", &callable.0.value)?;
        let key = key
            .as_string()
            .and_then(|key| key.parse().ok())
            .and_then(HostKey::new)
            .ok_or_else(|| JsValue::from_str("dispatchFor did not answer a host-value key"))?;
        SIGNATURES.with(|cell| cell.borrow_mut().insert(key, signature.clone()));
        Ok(key)
    }

    fn leaf(
        &self,
        leaf: &Js<View<'_>>,
        kind: LeafKind,
    ) -> Result<(LeafWire, Option<Owner>), JsValue> {
        let found = self.adapter_call("leafOf", &leaf.0.value)?;
        let bytes = Uint8Array::new(&Reflect::get(&found, &JsValue::from_str("bytes"))?).to_vec();
        let value = InboundValue::decode(bytes.as_slice())
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let owner = match Reflect::get(&found, &JsValue::from_str("cloned"))?.as_string() {
            Some(cloned) => Some(Owner::Handle(
                cloned
                    .parse()
                    .ok()
                    .and_then(TableKey::new)
                    .ok_or_else(|| JsValue::from_str("cloned handle key is not a live key"))?,
            )),
            None => None,
        };
        Ok((leaf_wire(kind, value)?, owner))
    }

    fn release(&self, owner: Owner) {
        match owner {
            Owner::Callable(key) => {
                sys_wasm::release_host_callable(key.get());
                forget_signature(key);
            }
            // Best effort: a key the engine never saw has no other owner.
            Owner::Handle(key) => {
                let _ = handle_cffi::release_handle(key.get());
            }
        }
    }
}

struct View<'a> {
    value: JsValue,
    cx: &'a Cx,
}

impl View<'_> {
    /// What an object is: a leaf the runtime owns, an instance of a class the
    /// typemap knows, a plain object, or something else.
    fn object_kind(&self) -> Result<JsKind, JsValue> {
        if let Some(kind) = self.cx.leaf_kind_of(&self.value)? {
            return Ok(JsKind::Leaf(kind));
        }
        if let Some(fqn) = self
            .cx
            .adapter_call("classOf", &self.value)?
            .as_string()
            .filter(|fqn| !fqn.is_empty())
        {
            return Ok(JsKind::Instance(TypeName::from_dotted_path(&fqn)));
        }
        if self.cx.is_plain_object(&self.value)? {
            return Ok(JsKind::PlainObject);
        }
        Ok(JsKind::Other)
    }
}

impl JsView for View<'_> {
    type Error = JsValue;

    fn kind(&self) -> Result<JsKind, JsValue> {
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
        if value.is_string() {
            return Ok(JsKind::String);
        }
        if Array::is_array(value) {
            return Ok(JsKind::Array);
        }
        if value.is_instance_of::<Uint8Array>() {
            return Ok(JsKind::Bytes);
        }
        if value.is_function() {
            // A function the runtime knows as a BAML closure crosses as its
            // handle; any other is a host callable.
            return Ok(match self.cx.leaf_kind_of(value)? {
                Some(kind) => JsKind::Leaf(kind),
                None => JsKind::Function,
            });
        }
        if value.is_object() {
            return self.object_kind();
        }
        Ok(JsKind::Other)
    }

    fn elements(&self) -> Result<Vec<Self>, JsValue> {
        Ok(Array::from(&self.value)
            .iter()
            .map(|value| View { value, cx: self.cx })
            .collect())
    }

    fn entries(&self) -> Result<Vec<(String, Self)>, JsValue> {
        Ok(entries_of(&self.value)?
            .into_iter()
            .map(|(name, value)| (name, View { value, cx: self.cx }))
            .collect())
    }

    fn type_args(&self) -> Result<Option<Vec<RuntimeTy>>, JsValue> {
        let found = self.cx.adapter_call("typeArgsOf", &self.value)?;
        if found.is_null_or_undefined() {
            return Ok(None);
        }
        Array::from(&found)
            .iter()
            .map(|bytes| {
                let ty = BamlTy::decode(Uint8Array::new(&bytes).to_vec().as_slice())
                    .map_err(|error| JsValue::from_str(&error.to_string()))?;
                bridge_ctypes::proto_ty_to_runtime_ty(&ty)
                    .map_err(|error| JsValue::from_str(&error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }

    fn text(&self) -> Result<Cow<'_, str>, JsValue> {
        well_formed(&self.value).map(Cow::Owned)
    }

    fn bytes(&self) -> Result<Cow<'_, [u8]>, JsValue> {
        Ok(Cow::Owned(Uint8Array::new(&self.value).to_vec()))
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
