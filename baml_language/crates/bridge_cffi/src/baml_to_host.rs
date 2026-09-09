//! Shared `Result<BexExternalValue, RuntimeError>` → `BamlOutboundResult`
//! translation: encoding the engine's BEV result into the host-facing
//! `BamlOutboundResult` envelope.
//!
//! The whole classify-and-encode lives here, in the bridge — not in bex.
//! C, Python, Node, Java and Wasm call [`prepare_call`] and
//! [`invoke_prepared`], so the
//! `catch_unwind` → `SdkPanic` boundary and the error/panic routing are
//! defined exactly once. Every result — ok value, thrown error, panic, and
//! pre-call host-boundary failure — leaves the bridge as one envelope; there is
//! no separate error channel.
//!
//! Routing recovers the panic-vs-error distinction the same way the VM does
//! internally: by namespace. A thrown `BexExternalValue::Instance` whose
//! `class_name` is under `baml.panics.*` is a panic; anything else is an
//! error. Host-originated infra failures that never entered the VM as throws
//! are *synthesized* into the `baml.errors.*` / `baml.panics.SdkPanic` classes
//! added in 31d-phase3.

use std::{panic::AssertUnwindSafe, sync::Arc};

use bex_project::{
    Bex, BexArgs, BexExternalValue, EngineError, FunctionCallContext, RuntimeError,
    UnhandledSpawnError,
};
use bridge_ctypes::{
    CffiHandleTableEntry, CffiHandleTableOptions, EncodedTransfer, HANDLE_TABLE, OutboundEncoder,
    baml_bridge::cffi::{
        BamlOutboundError, BamlOutboundPanic, BamlOutboundResult, baml_outbound_result,
    },
};
use futures::future::FutureExt;
use indexmap::IndexMap;
use prost::Message;

use crate::error::BridgeError;

/// Namespace prefix marking a thrown value as a panic rather than an error.
const PANIC_NS_PREFIX: &str = "baml.panics.";

const GENERIC_SDK_ERROR_CLASS: &str = "baml.errors.GenericSdkError";
const INVALID_ARGUMENT_CLASS: &str = "baml.errors.InvalidArgument";
const COMPILATION_ERROR_CLASS: &str = "baml.errors.CompilationError";
const ACCESS_ERROR_CLASS: &str = "baml.errors.AccessError";
const TYPE_MISMATCH_CLASS: &str = "baml.errors.TypeMismatch";
const SDK_PANIC_CLASS: &str = "baml.panics.SdkPanic";
const EXIT_CLASS: &str = "baml.panics.Exit";

/// Build a one-field class instance (`class_name { field: value }`).
fn one_field_instance(class_name: &str, field: &str, value: BexExternalValue) -> BexExternalValue {
    let mut fields = IndexMap::new();
    fields.insert(field.to_string(), value);
    BexExternalValue::Instance {
        class_name: class_name.to_string(),
        type_args: vec![],
        fields,
    }
}

/// Build a `class_name { message: <message> }` instance — the shape of every
/// synthesized `baml.errors.*` / `baml.panics.SdkPanic` infra class.
fn message_instance(class_name: &str, message: String) -> BexExternalValue {
    one_field_instance(
        class_name,
        "message",
        BexExternalValue::String(message.into()),
    )
}

/// True iff the (possibly union-wrapped) thrown value is a `baml.panics.*`
/// instance — the same namespace rule the VM uses via pointer identity, just
/// re-evaluated on the FQN string at the encode boundary.
fn is_panic_value(value: &BexExternalValue) -> bool {
    match value {
        BexExternalValue::Instance { class_name, .. } => class_name.starts_with(PANIC_NS_PREFIX),
        BexExternalValue::Union { value, .. } => is_panic_value(value),
        _ => false,
    }
}

/// Encode a `baml.panics.SdkPanic { message }` panic arm. Total: if even the
/// `SdkPanic` instance fails to encode, fall back to a value-less panic that
/// carries the message as a single trace line.
fn sdk_panic_arm(
    message: String,
    encoder: &mut OutboundEncoder<'_>,
) -> baml_outbound_result::Result {
    let value = message_instance(SDK_PANIC_CLASS, message.clone());
    match encoder.encode(&value) {
        Ok(ob) => baml_outbound_result::Result::Panic(BamlOutboundPanic {
            value: Some(ob),
            trace: Vec::new(),
            is_exit_panic: false,
            exit_code: 0,
        }),
        Err(_) => baml_outbound_result::Result::Panic(BamlOutboundPanic {
            value: None,
            trace: vec![message],
            is_exit_panic: false,
            exit_code: 0,
        }),
    }
}

/// Encode a synthesized `baml.errors.*` infra value as the `error` arm
/// (empty trace — these never entered the VM). Falls back to an `SdkPanic` if
/// the value somehow fails to encode.
///
/// Fed by host-originated failures that never reached the VM as a throw:
/// [`RuntimeError`] direct arms and pre-call [`BridgeError`]s, each mapped to a
/// `baml.errors.*` class by its caller.
fn infra_error_arm(
    value: BexExternalValue,
    encoder: &mut OutboundEncoder<'_>,
) -> baml_outbound_result::Result {
    match encoder.encode(&value) {
        Ok(ob) => baml_outbound_result::Result::Error(BamlOutboundError {
            value: Some(ob),
            trace: Vec::new(),
        }),
        Err(e) => sdk_panic_arm(format!("failed to encode error value: {e}"), encoder),
    }
}

/// Encode an actually-thrown value (from `UnhandledThrow`), routed to the
/// `error` or `panic` arm by namespace, carrying the pre-rendered `trace`.
fn thrown_arm(
    value: BexExternalValue,
    trace: Vec<String>,
    encoder: &mut OutboundEncoder<'_>,
) -> baml_outbound_result::Result {
    let is_panic = is_panic_value(&value);
    match encoder.encode(&value) {
        Ok(ob) if is_panic => baml_outbound_result::Result::Panic(BamlOutboundPanic {
            value: Some(ob),
            trace,
            is_exit_panic: false,
            exit_code: 0,
        }),
        Ok(ob) => baml_outbound_result::Result::Error(BamlOutboundError {
            value: Some(ob),
            trace,
        }),
        Err(e) => sdk_panic_arm(format!("failed to encode thrown value: {e}"), encoder),
    }
}

/// Translate the engine's `Result<BexExternalValue, RuntimeError>` into the
/// `BamlOutboundResult` envelope. The only genuinely new logic is the
/// namespace check (error vs panic) and synthesizing infra classes for the
/// host-originated failures that never entered the VM as throws; value
/// materialization is reused verbatim via [`OutboundEncoder`].
pub fn result_to_outbound(
    result: Result<BexExternalValue, RuntimeError>,
    options: &CffiHandleTableOptions,
) -> BamlOutboundResult {
    let mut encoder = OutboundEncoder::new(*options);
    let result = encode_result_to_outbound(result, &mut encoder);
    encoder.finish(result).into_unreceipted()
}

/// Receipt-ready classification. Failed value encodes roll back before the
/// fallback error is encoded into this aggregate.
pub fn encode_result_to_outbound(
    result: Result<BexExternalValue, RuntimeError>,
    encoder: &mut OutboundEncoder<'_>,
) -> BamlOutboundResult {
    let inner = match result {
        Ok(value) => match encoder.encode(&value) {
            Ok(ob) => baml_outbound_result::Result::Ok(ob),
            Err(e) => sdk_panic_arm(format!("failed to encode return value: {e}"), encoder),
        },

        // Clean `baml.sys.exit(code)` — the engine pulled the code out before
        // it could reach `UnhandledThrow`. Synthesize the `baml.panics.Exit`
        // value and set the exit discriminator so the host exits the process.
        Err(RuntimeError::Engine(EngineError::Exit { code })) => {
            let value = one_field_instance(EXIT_CLASS, "code", BexExternalValue::Int(code));
            match encoder.encode(&value) {
                Ok(ob) => baml_outbound_result::Result::Panic(BamlOutboundPanic {
                    value: Some(ob),
                    trace: Vec::new(),
                    is_exit_panic: true,
                    exit_code: code,
                }),
                Err(e) => sdk_panic_arm(format!("failed to encode exit value: {e}"), encoder),
            }
        }

        // 🟩 user/stdlib throws and the 🟦 `Cancelled` class-tag — the value is
        // already a `BexExternalValue`; route by namespace, carry the trace.
        Err(RuntimeError::Engine(EngineError::UnhandledThrow { value, trace })) => {
            let lines = bridge_ctypes::format_traceback_lines(
                trace
                    .iter()
                    .map(|f| (f.file_path.as_str(), f.error_line, f.function_name.as_str())),
            );
            thrown_arm(*value, lines, encoder)
        }

        // 🟥 A value/type mismatch at the call boundary is a *caller* type error,
        // not an SDK panic — route it to a structured `baml.errors.TypeMismatch`
        // (an `error` arm) so each host SDK can surface it as its native
        // type-error (Python `TypeError`) rather than an opaque `SdkPanic`. This
        // covers inbound-generics Gate-A failures (a `TypeVar` with no inference
        // evidence that must be specified, conflicting variance occurrences) and
        // ordinary argument-conversion mismatches alike.
        Err(RuntimeError::Engine(EngineError::TypeMismatch { message })) => {
            infra_error_arm(message_instance(TYPE_MISMATCH_CLASS, message), encoder)
        }

        Err(RuntimeError::Engine(err @ EngineError::FunctionNotFound { .. })) => infra_error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            encoder,
        ),

        // Every other 🟥 engine/VM-internal failure → one opaque `SdkPanic`,
        // its `Display` (incl. any formatted VM trace) carried as `message`.
        Err(RuntimeError::Engine(engine_err)) => sdk_panic_arm(engine_err.to_string(), encoder),

        // 🟥 `RuntimeError` direct arms → fine-grained `baml.errors.*`.
        Err(RuntimeError::Other(s)) => {
            infra_error_arm(message_instance(GENERIC_SDK_ERROR_CLASS, s), encoder)
        }
        Err(err @ RuntimeError::InvalidArgument { .. }) => infra_error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            encoder,
        ),
        Err(RuntimeError::Compilation { message }) => {
            infra_error_arm(message_instance(COMPILATION_ERROR_CLASS, message), encoder)
        }
        Err(RuntimeError::Access(inner)) => infra_error_arm(
            message_instance(ACCESS_ERROR_CLASS, inner.to_string()),
            encoder,
        ),
    };

    BamlOutboundResult {
        result: Some(inner),
    }
}

pub fn unhandled_spawn_error_to_outbound(
    error: UnhandledSpawnError,
) -> EncodedTransfer<'static, Vec<u8>> {
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let trace = bridge_ctypes::format_traceback_lines(error.trace.iter().map(|frame| {
        (
            frame.file_path.as_str(),
            frame.error_line,
            frame.function_name.as_str(),
        )
    }));
    let result = BamlOutboundResult {
        result: Some(thrown_arm(error.value, trace, &mut encoder)),
    };
    encoder
        .finish(result)
        .map_payload(|result| result.encode_to_vec())
}

/// Encode a pre-call host-boundary [`BridgeError`] as `BamlOutboundResult`
/// envelope bytes (32c). These failures never entered the VM, so they carry an
/// empty trace and ride the *same* decode path as engine errors via
/// `decode_call_result` — surfacing host-side as a structured
/// `BamlError(baml.errors.*)`, indistinguishable from an engine failure.
///
/// The `Runtime` arm reuses [`result_to_outbound`] verbatim (the fine-grained
/// `baml.errors.*` / `SdkPanic` mapping from 31d/31e); the remaining pre-call
/// variants map to `InvalidArgument` (bad function name / arguments) or
/// `GenericSdkError` (setup / internal), reusing the same synthesis helpers —
/// no new construction logic.
pub fn error_to_outbound(err: BridgeError) -> Vec<u8> {
    error_to_outbound_encoded(err).into_unreceipted()
}

pub fn error_to_outbound_encoded(err: BridgeError) -> EncodedTransfer<'static, Vec<u8>> {
    error_to_outbound_message(err).map_payload(|result| result.encode_to_vec())
}

pub(crate) fn error_to_outbound_message(
    err: BridgeError,
) -> EncodedTransfer<'static, BamlOutboundResult> {
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let inner = match err {
        // Reuse the engine-error mapping verbatim for the wrapped RuntimeError.
        BridgeError::Runtime(rt) => encode_result_to_outbound(Err(rt), &mut encoder)
            .result
            .expect("result_to_outbound always sets the result oneof"),

        // Bad function name / arguments → InvalidArgument.
        err @ (BridgeError::Ctypes(_)
        | BridgeError::MissingCallTarget
        | BridgeError::InvalidInvocation(_)
        | BridgeError::FunctionHandleTypeArgs
        | BridgeError::FunctionNotFound { .. }
        | BridgeError::MissingArgument { .. }
        | BridgeError::InvalidCallId) => infra_error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            &mut encoder,
        ),

        // Setup / internal host failures (NotInitialized, ProjectNotInitialized,
        // LockPoisoned, NotImplemented, DuplicateCallId, Internal) → GenericSdkError.
        err => infra_error_arm(
            message_instance(GENERIC_SDK_ERROR_CLASS, err.to_string()),
            &mut encoder,
        ),
    };

    let result = BamlOutboundResult {
        result: Some(inner),
    };
    encoder.finish(result)
}

/// Render a caught panic payload into a message for `baml.panics.SdkPanic`.
fn panic_message(panic_info: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        format!("Panic: {s}")
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        format!("Panic: {s}")
    } else {
        "Panic: unknown".to_string()
    }
}

/// Encode a caught panic payload as `BamlOutboundResult` envelope bytes.
///
/// Used by the C-ABI entry point's *outer* `catch_unwind` (the safety net for a
/// panic during *encoding*, which must not cross the C boundary): instead of an
/// opaque string, the panic rides the same `baml.panics.SdkPanic` ⇒
/// `BamlOutboundPanic` envelope as a call-time panic — uniform with every other
/// result.
pub fn panic_to_outbound(panic_info: &(dyn std::any::Any + Send)) -> Vec<u8> {
    panic_to_outbound_message(panic_info)
        .map_payload(|result| result.encode_to_vec())
        .into_unreceipted()
}

pub(crate) fn panic_to_outbound_message(
    panic_info: &(dyn std::any::Any + Send),
) -> EncodedTransfer<'static, BamlOutboundResult> {
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let result = BamlOutboundResult {
        result: Some(sdk_panic_arm(panic_message(panic_info), &mut encoder)),
    };
    encoder.finish(result)
}

/// Fully prepared calls own their temporary receiver/type pins before the
/// platform yields to an executor. SDK reference disposal cannot invalidate
/// an already prepared call. The engine still checks runtime provenance.
pub struct PreparedCall {
    target: PreparedTarget,
    args: BexArgs,
    context: FunctionCallContext,
}

enum PreparedTarget {
    Named(String),
    Callable(bex_project::Handle),
    ConcreteMethod(bex_project::ConcreteMethodCall),
    Method {
        view: Arc<bex_project::InterfaceValue>,
        member: String,
        type_args: Vec<bex_project::TypeArgument>,
    },
}

pub fn prepare_call(bytes: &[u8]) -> Result<PreparedCall, BridgeError> {
    let result = prepare_call_inner(bytes);
    if result.is_err() {
        // Preparation holds no heap permit. A rejected call never reaches the
        // engine's normal safepoint, so flush its queued host releases here.
        bex_project::host_release_dispatch::drain();
    }
    result
}

fn prepare_call_inner(bytes: &[u8]) -> Result<PreparedCall, BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget};
    let call = CallFunctionArgs::decode(bytes).map_err(bridge_ctypes::CtypesError::from)?;
    // A parsed envelope transfers the whole argument batch, even if target or
    // type validation fails before any value is decoded.
    let inputs = bridge_ctypes::InboundTransfer::capture_kwargs(&call.kwargs, &HANDLE_TABLE);
    if call.call_id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    let target = call.call_target.ok_or(BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !call.type_args.is_empty() {
        return Err(BridgeError::FunctionHandleTypeArgs);
    }
    if matches!(
        target,
        CallTarget::InterfaceMethod(_) | CallTarget::ConcreteMethod(_)
    ) && !call.type_args.is_empty()
    {
        return Err(BridgeError::InvalidInvocation(
            "method type arguments belong to its target".into(),
        ));
    }
    let bindings = bridge_ctypes::proto_ty_args_to_named(&call.type_args)?;
    let context = crate::function_call_context_builder(bex_project::CallId(call.call_id))
        .with_type_bindings(bindings)
        .build();
    // Pin target references before decoding values or yielding to the host executor.
    let target = match target {
        CallTarget::FunctionName(name) => PreparedTarget::Named(name),
        CallTarget::FunctionHandle(key) => {
            let entry = HANDLE_TABLE.resolve(key).ok_or_else(|| {
                BridgeError::InvalidInvocation("callable reference is closed".into())
            })?;
            let CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                kind: bex_project::TaggedHeapHandleKind::Callable,
                heap_handle,
                ..
            }) = &*entry
            else {
                return Err(BridgeError::InvalidInvocation(
                    "reference is not a callable".into(),
                ));
            };
            // Required argument names belong to the checked callable descriptor
            // retained in this entry; host-supplied type decoration is unused.
            let CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                ty: bex_project::RuntimeTy::Function { params, .. },
                ..
            }) = &*entry
            else {
                return Err(BridgeError::InvalidInvocation(
                    "callable has no realized signature".into(),
                ));
            };
            let kwargs = inputs.decode_kwargs(call.kwargs)?;
            let args = partition_callable_args(
                key,
                params.iter().enumerate().map(|(index, p)| {
                    (
                        p.name
                            .as_ref()
                            .map_or_else(|| format!("arg{index}"), ToString::to_string),
                        p.is_required(),
                    )
                }),
                kwargs.into_iter().collect(),
            )?;
            return Ok(PreparedCall {
                target: PreparedTarget::Callable(heap_handle.clone()),
                args,
                context,
            });
        }
        CallTarget::InterfaceMethod(method) => {
            let entry = HANDLE_TABLE.resolve(method.view).ok_or_else(|| {
                BridgeError::InvalidInvocation("interface reference is closed".into())
            })?;
            let CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::Interface(view)) = &*entry
            else {
                return Err(BridgeError::InvalidInvocation(
                    "reference is not an interface view".into(),
                ));
            };
            if method.type_args.iter().any(|arg| !arg.type_var.is_empty()) {
                return Err(BridgeError::InvalidInvocation(
                    "method type arguments are positional; type_var must be empty".into(),
                ));
            }
            let type_args = method
                .type_args
                .iter()
                .map(bridge_ctypes::proto_type_argument)
                .collect::<Result<Vec<_>, _>>()?;
            PreparedTarget::Method {
                view: view.clone(),
                member: method.member,
                type_args,
            }
        }
        CallTarget::ConcreteMethod(method) => {
            let entry = HANDLE_TABLE.resolve(method.receiver).ok_or_else(|| {
                BridgeError::InvalidInvocation("concrete reference is closed".into())
            })?;
            let CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                kind: bex_project::TaggedHeapHandleKind::ConcreteObject,
                heap_handle,
                ..
            }) = &*entry
            else {
                return Err(BridgeError::InvalidInvocation(
                    "reference is not a concrete object".into(),
                ));
            };
            if method.class_name.is_empty() || method.member.is_empty() {
                return Err(BridgeError::InvalidInvocation(
                    "concrete method requires a class and member".into(),
                ));
            }
            if method.type_args.iter().any(|arg| !arg.type_var.is_empty()) {
                return Err(BridgeError::InvalidInvocation(
                    "method type arguments are positional; type_var must be empty".into(),
                ));
            }
            use bridge_ctypes::baml_bridge::cffi::concrete_method_target::Dispatch;
            let interface_pattern = match &method.dispatch {
                Some(Dispatch::InterfacePattern(pattern)) => {
                    let pattern = bridge_ctypes::proto_ty_to_runtime_ty(pattern)?;
                    if !matches!(pattern, bex_project::RuntimeTy::Interface(..)) {
                        return Err(BridgeError::InvalidInvocation(
                            "concrete method obligation must be an interface".into(),
                        ));
                    }
                    Some(pattern)
                }
                Some(Dispatch::Inherent(true)) => None,
                _ => {
                    return Err(BridgeError::InvalidInvocation(
                        "concrete method requires explicit dispatch".into(),
                    ));
                }
            };
            let type_args = method
                .type_args
                .iter()
                .map(bridge_ctypes::proto_type_argument)
                .collect::<Result<Vec<_>, _>>()?;
            PreparedTarget::ConcreteMethod(bex_project::ConcreteMethodCall {
                receiver: heap_handle.clone(),
                class: bex_project::TypeName::from_dotted_path(&method.class_name),
                interface_pattern,
                member: method.member,
                type_args,
            })
        }
    };
    let args = inputs.decode_kwargs(call.kwargs)?.into();
    Ok(PreparedCall {
        target,
        args,
        context,
    })
}

pub async fn invoke_prepared(runtime: Arc<dyn Bex>, call: PreparedCall) -> Vec<u8> {
    invoke_prepared_encoded(runtime, call)
        .await
        .into_unreceipted()
}

/// Invoke through the same classifier as every native adapter, retaining the
/// encoded result until its transport stages and adopts/discards ownership.
pub async fn invoke_prepared_encoded(
    runtime: Arc<dyn Bex>,
    call: PreparedCall,
) -> EncodedTransfer<'static, Vec<u8>> {
    let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
    let _route = crate::register_active_call_runtime(call.context.host_call_id.0, &runtime);
    let caught = AssertUnwindSafe(async move {
        match call.target {
            PreparedTarget::Named(name) => {
                runtime.call_function(&name, call.args, call.context).await
            }
            PreparedTarget::Callable(handle) => {
                runtime.call_callable(handle, call.args, call.context).await
            }
            PreparedTarget::ConcreteMethod(target) => {
                let mut args = call.args.required;
                args.extend(call.args.optional);
                runtime
                    .call_concrete_method(target, args, call.context)
                    .await
            }
            PreparedTarget::Method {
                view,
                member,
                type_args,
            } => {
                let mut args = call.args.required;
                args.extend(call.args.optional);
                runtime
                    .call_interface_method(view, &member, type_args, args, call.context)
                    .await
            }
        }
    })
    .catch_unwind()
    .await;
    let result = match caught {
        Ok(result) => encode_result_to_outbound(result, &mut encoder),
        Err(panic) => BamlOutboundResult {
            result: Some(sdk_panic_arm(panic_message(panic.as_ref()), &mut encoder)),
        },
    }
    .encode_to_vec();
    encoder.finish(result)
}

fn partition_callable_args(
    handle_key: u64,
    params: impl IntoIterator<Item = (String, bool)>,
    mut supplied: IndexMap<String, BexExternalValue>,
) -> Result<BexArgs, BridgeError> {
    let mut required = IndexMap::new();
    let mut optional = IndexMap::new();
    for (name, is_required) in params {
        let value = supplied.shift_remove(&name);
        if !is_required {
            if let Some(value) = value {
                optional.insert(name, value);
            }
            continue;
        }
        let Some(value) = value else {
            return Err(BridgeError::MissingArgument {
                function: format!("function handle {handle_key}"),
                parameter: name,
            });
        };
        required.insert(name, value);
    }
    optional.extend(supplied);
    Ok(BexArgs { required, optional })
}

#[cfg(test)]
mod tests {
    use bridge_ctypes::baml_bridge::cffi::{
        BamlOutboundResult, baml_outbound_result, baml_outbound_value,
    };
    use indexmap::IndexMap;
    use prost::Message;

    use super::{error_to_outbound, partition_callable_args};
    use crate::BridgeError;

    #[test]
    fn staged_thrown_host_value_retains_registration_until_discard() {
        use bex_project::{
            BexExternalValue, EngineError, HostValueArc, HostValueKind, RuntimeError,
        };
        use bridge_ctypes::{
            CffiHandleTableOptions, HANDLE_TABLE, OutboundEncoder, TransferSession,
        };
        use std::sync::Arc;

        let host = HostValueArc::new(9101, HostValueKind::Opaque);
        let weak = Arc::downgrade(&host);
        let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
        let result = super::encode_result_to_outbound(
            Err(RuntimeError::Engine(EngineError::UnhandledThrow {
                value: Box::new(BexExternalValue::HostValue(host)),
                trace: vec![],
            })),
            &mut encoder,
        );
        assert!(matches!(
            result.result,
            Some(baml_outbound_result::Result::Error(_))
        ));
        let session = TransferSession::new(&HANDLE_TABLE);
        let (receipt, _) = session
            .stage(
                encoder
                    .finish(result)
                    .map_payload(|value| value.encode_to_vec()),
            )
            .unwrap()
            .handoff();
        assert!(weak.upgrade().is_some());
        session.discard(receipt).unwrap();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn failed_return_encoding_stages_only_the_fallback_panic() {
        use bex_project::{BexExternalValue, RuntimeTy};
        use bridge_ctypes::{
            CffiHandleTableOptions, HANDLE_TABLE, OutboundEncoder, TransferSession,
        };
        use std::sync::Arc;

        let resource = Arc::new(());
        let weak = Arc::downgrade(&resource);
        let value = BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items: vec![
                BexExternalValue::RustData(resource),
                BexExternalValue::union(
                    BexExternalValue::Bool(true),
                    [RuntimeTy::int(), RuntimeTy::float()],
                    RuntimeTy::bool(),
                ),
            ],
        };
        let mut encoder = OutboundEncoder::new(CffiHandleTableOptions::for_wire());
        let result = super::encode_result_to_outbound(Ok(value), &mut encoder);
        assert!(matches!(
            result.result,
            Some(baml_outbound_result::Result::Panic(_))
        ));
        assert!(
            weak.upgrade().is_none(),
            "failed root's resource must not survive in fallback ledger"
        );
        let session = TransferSession::new(&HANDLE_TABLE);
        let (receipt, _) = session.stage(encoder.finish(result)).unwrap().handoff();
        session.adopt(receipt, &[]).unwrap();
        assert_eq!(session.pending_count(), 0);
    }

    #[test]
    fn function_handle_type_args_are_classified_as_invalid_argument() {
        let encoded = error_to_outbound(BridgeError::FunctionHandleTypeArgs);
        let envelope = BamlOutboundResult::decode(encoded.as_slice()).unwrap();
        let Some(baml_outbound_result::Result::Error(error)) = envelope.result else {
            panic!("expected an error envelope");
        };
        let Some(baml_outbound_value::Value::ClassValue(class)) =
            error.value.and_then(|value| value.value)
        else {
            panic!("expected a structured error class");
        };
        assert_eq!(class.name, "baml.errors.InvalidArgument");
    }

    #[test]
    fn callable_missing_required_argument_is_classified_as_invalid_argument() {
        let Err(err) =
            partition_callable_args(42, [("required_value".to_string(), true)], IndexMap::new())
        else {
            panic!("omitting a required callable argument must fail");
        };
        let encoded = error_to_outbound(err);
        let envelope = BamlOutboundResult::decode(encoded.as_slice()).unwrap();
        let Some(baml_outbound_result::Result::Error(error)) = envelope.result else {
            panic!("expected an error envelope");
        };
        let Some(baml_outbound_value::Value::ClassValue(class)) =
            error.value.and_then(|value| value.value)
        else {
            panic!("expected a structured error class");
        };
        assert_eq!(class.name, "baml.errors.InvalidArgument");
        let message = class
            .fields
            .iter()
            .find(|field| field.key == "message")
            .and_then(|field| field.value.as_ref())
            .and_then(|value| value.value.as_ref());
        assert!(
            matches!(
                message,
                Some(baml_outbound_value::Value::StringValue(message))
                    if message.contains("required_value")
            ),
            "missing-argument envelope should name the omitted callable parameter"
        );
    }
}
