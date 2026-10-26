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
use baml_type::{MediaKind, Name, TypeName};
use bridge_cffi::handle_cffi;
use bridge_ctypes::{
    HANDLE_TABLE, HostKey, TableKey,
    baml_bridge::cffi::{BamlTy, CallFunctionArgs, InboundValue, call_function_args::CallTarget},
};
use bridge_encode::{
    Completion, Encoded, Error, Facts, FunctionSig, Host, LeafKind, LeafWire, Owner, Owners, Param,
    Root, SlotTy, StatedTy, encode, encode_arguments,
    js::{Js, JsElements, JsEntries, JsKind, JsView},
    state_type_args,
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
    static SIGNATURES: RefCell<HashMap<HostKey, StatedTy>> = RefCell::new(HashMap::new());
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
    let mut call = CallFunctionArgs::decode(args_proto).map_err(|error| {
        crate::errors::setup_error(
            crate::errors::INVALID_ARGUMENT,
            format!("malformed CallFunctionArgs: {error}"),
        )
    })?;
    if kwargs.is_null_or_undefined() && cancel.is_null_or_undefined() && call.type_args.is_empty() {
        return Ok(args_proto.to_vec());
    }
    let facts = facts();
    let cx = Cx::new(sync)?;
    let mut owners = Owners::new(&cx);
    let (target, signature) = target_signature(&facts, call.call_target.as_ref());
    let outcome = (|| -> Result<(), JsValue> {
        // The explicit type arguments as the wrappers hold them, restated
        // with references of the call's own: the wrappers keep theirs.
        let type_args: Vec<(String, StatedTy)> =
            bridge_ctypes::proto_ty_args_to_stated(&call.type_args, &HANDLE_TABLE)
                .map_err(|error| JsValue::from_str(&error.to_string()))?
                .into_iter()
                .collect();
        call.type_args = state_type_args(&type_args, &mut owners)?;
        if !cancel.is_null_or_undefined() {
            let encoded = encode(
                &facts,
                cx.value(cancel.clone()),
                &SlotTy::Unknown,
                Root::Argument("$baml.cancel".into()),
                &mut owners,
            )
            .map_err(|error| raise(&target, error))?;
            call.invocation.get_or_insert_with(Default::default).cancel = Some(encoded);
        }
        if kwargs.is_null_or_undefined() {
            return Ok(());
        }
        let arguments = keys_of(kwargs)?
            .into_iter()
            .map(|name| {
                let value = Reflect::get(kwargs, &JsValue::from_str(&name))?;
                Ok((name, cx.value(value)))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let bindings = type_args
            .iter()
            .map(|(name, ty)| (Name::new(name), ty.clone()))
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
    let ret = match signature_of_call(call_id) {
        Some(StatedTy::Function { ret, .. }) => *ret,
        Some(_) | None => StatedTy::Unknown,
    };
    let completion = encode_result(value, &ret);
    let outcome = match &completion {
        Completion::Wire(_) => "ok",
        Completion::HostError(_)
        | Completion::ContractViolation(_)
        | Completion::BridgeFailure(_) => "error",
    };
    let wire = completion.into_wire(Encoded::Returned)?;
    sys_wasm::complete_host_call(call_id, &wire.encode_to_vec());
    Ok(outcome.to_string())
}

/// Complete an in-flight host call with a thrown BAML value, encoded against
/// the throws type of the slot the callable was bound to; a `baml.panics.*`
/// value is a panic whatever the slot declares.
#[wasm_bindgen(js_name = completeWebHostCallThrow)]
pub fn complete_web_host_call_throw(
    call_id: u32,
    value: JsValue,
    is_panic: bool,
) -> Result<bool, JsValue> {
    // A panic value is encoded against `unknown`: against the declared throws
    // type it would be refused as a contract violation before the engine's
    // panic exemption applies.
    let throws = match signature_of_call(call_id) {
        _ if is_panic => StatedTy::Unknown,
        Some(StatedTy::Function { throws, .. }) => *throws,
        Some(_) | None => StatedTy::Unknown,
    };
    let wire = encode_result(value, &throws).into_wire(Encoded::Thrown)?;
    Ok(sys_wasm::complete_host_call(call_id, &wire.encode_to_vec()))
}

/// The function type the callable dispatched as `call_id` was bound to.
fn signature_of_call(call_id: u32) -> Option<StatedTy> {
    let key = sys_wasm::host_callable_key_for_call(call_id)?;
    SIGNATURES.with(|cell| cell.borrow().get(&key).cloned())
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
        &SlotTy::Unknown,
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
fn encode_result(value: JsValue, slot: &StatedTy) -> Completion<JsValue> {
    let facts = facts();
    let cx = match Cx::new(false) {
        Ok(cx) => cx,
        // The bridge's own setup failing (no adapter registered) is the
        // bridge's failure, never the callable raising.
        Err(error) => return Completion::BridgeFailure(bridge_encode::BridgeFault::Host(error)),
    };
    let mut owners = Owners::new(&cx);
    let result = encode(
        &facts,
        cx.value(value),
        &SlotTy::from(slot),
        Root::Result,
        &mut owners,
    );
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
        ret: SlotTy::Unknown,
        throws: SlotTy::Unknown,
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
                            bex_project::AnnotationTy::Function {
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
                            ty: param.ty.as_stated().into(),
                            has_default: param.is_optional(),
                        })
                        .collect(),
                    ret: ret.as_stated().into(),
                    throws: throws.as_stated().into(),
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
/// `Object.keys(value)`: the own enumerable string keys, in insertion order.
/// Only the keys are read here; a value (a getter, a Proxy trap) is read when
/// the walker pulls its entry, after the budget charged it.
fn keys_of(value: &JsValue) -> Result<Vec<String>, JsValue> {
    // `typeof`, not `instanceof Object`: the runtime decodes BAML maps into
    // prototype-free objects, which are objects all the same.
    if !value.is_object() {
        return Err(JsValue::from_str("expected an object"));
    }
    let object: &Object = value.unchecked_ref();
    Object::keys(object)
        .iter()
        .map(|key| well_formed(&key))
        .collect()
}

/// What one encode shares across every value it visits: the adapter.
struct Cx {
    adapter: Object,
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
        Ok(Self { adapter, sync })
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

    /// Is `value` a plain object? A realm question (another realm's
    /// `Object.prototype` is not this one's), so the runtime's adapter
    /// answers it in JavaScript.
    fn is_plain_object(&self, value: &JsValue) -> Result<bool, JsValue> {
        self.adapter_call("isPlainObject", value)?
            .as_bool()
            .ok_or_else(|| JsValue::from_str("isPlainObject did not answer a boolean"))
    }

    /// Is `value` a `Uint8Array`? A realm question like `isPlainObject`, so
    /// the adapter answers it.
    fn is_uint8_array(&self, value: &JsValue) -> Result<bool, JsValue> {
        self.adapter_call("isUint8Array", value)?
            .as_bool()
            .ok_or_else(|| JsValue::from_str("isUint8Array did not answer a boolean"))
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
        // The type as the wrapper holds it; the wire takes fresh references
        // to its runtime declarations at commit.
        (LeafKind::TypeValue, Some(Value::TyValue(ty))) => Ok(LeafWire::TypeValue(
            bridge_ctypes::proto_ty_to_stated(&ty, &HANDLE_TABLE)
                .map_err(|error| JsValue::from_str(&error.to_string()))?,
        )),
        (LeafKind::Handle | LeafKind::Media(_) | LeafKind::Prompt | LeafKind::TypeValue, _) => {
            Err(JsValue::from_str(&format!(
                "the host adapter serialized a {kind:?} leaf as another kind"
            )))
        }
    }
}

impl Host<Js<View<'_>>> for Cx {
    fn register(&self, callable: &Js<View<'_>>, signature: &StatedTy) -> Result<HostKey, JsValue> {
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

    fn reference(&self, declaration: &bex_project::Handle) -> Result<TableKey, JsValue> {
        Ok(
            HANDLE_TABLE.insert(bridge_ctypes::CffiHandleTableEntry::BexHeapHandle(
                declaration.clone(),
            )),
        )
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
        if self.cx.is_uint8_array(value)? {
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

    fn elements(&self) -> Result<JsElements<'_, Self>, JsValue> {
        // Elements are read as the walker pulls them: `length` sizes nothing,
        // so a sparse array of any length costs only what the walker's budget
        // lets it read. A hole reads as `undefined`.
        // Through the object protocol, never `Array.from`, which would copy
        // the whole array (through its iterator) before the walker's budget
        // saw one element.
        let length = Reflect::get(&self.value, &JsValue::from_str("length"))?
            .as_f64()
            .ok_or_else(|| JsValue::from_str("array length is not a number"))?;
        let length = u32::try_from(length as u64)
            .map_err(|_| JsValue::from_str("array length does not fit an index"))?;
        let array = self.value.clone();
        let cx = self.cx;
        Ok(Box::new((0..length).map(move |index| {
            Ok(View {
                value: Reflect::get_u32(&array, index)?,
                cx,
            })
        })))
    }

    fn entries(&self) -> Result<JsEntries<'_, Self>, JsValue> {
        let cx = self.cx;
        let object = self.value.clone();
        Ok(Box::new(keys_of(&self.value)?.into_iter().map(
            move |name| {
                let value = Reflect::get(&object, &JsValue::from_str(&name))?;
                Ok((name, View { value, cx }))
            },
        )))
    }

    fn type_args(&self) -> Result<Option<Vec<StatedTy>>, JsValue> {
        let found = self.cx.adapter_call("typeArgsOf", &self.value)?;
        if found.is_null_or_undefined() {
            return Ok(None);
        }
        Array::from(&found)
            .iter()
            .map(|bytes| {
                let ty = BamlTy::decode(Uint8Array::new(&bytes).to_vec().as_slice())
                    .map_err(|error| JsValue::from_str(&error.to_string()))?;
                bridge_ctypes::proto_ty_to_stated(&ty, &HANDLE_TABLE)
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
