//! Shared `Result<BexExternalValue, RuntimeError>` → `BamlOutboundResult`
//! translation: encoding the engine's BEV result into the host-facing
//! `BamlOutboundResult` envelope.
//!
//! The whole classify-and-encode lives here, in the bridge — not in bex.
//! Both the C-ABI entry point (`call_function` in `lib.rs`) and the PyO3 path
//! (`bridge_python`'s `runtime.rs`) call [`call_and_encode`], so the
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
    CffiHandleTableEntry, CffiHandleTableOptions, HANDLE_TABLE,
    baml_bridge::cffi::{
        BamlOutboundError, BamlOutboundPanic, BamlOutboundResult, baml_outbound_result,
    },
    external_to_outbound,
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
    options: &CffiHandleTableOptions,
) -> baml_outbound_result::Result {
    let value = message_instance(SDK_PANIC_CLASS, message.clone());
    match external_to_outbound(&value, options) {
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
    options: &CffiHandleTableOptions,
) -> baml_outbound_result::Result {
    match external_to_outbound(&value, options) {
        Ok(ob) => baml_outbound_result::Result::Error(BamlOutboundError {
            value: Some(ob),
            trace: Vec::new(),
        }),
        Err(e) => sdk_panic_arm(format!("failed to encode error value: {e}"), options),
    }
}

/// Encode an actually-thrown value (from `UnhandledThrow`), routed to the
/// `error` or `panic` arm by namespace, carrying the pre-rendered `trace`.
fn thrown_arm(
    value: BexExternalValue,
    trace: Vec<String>,
    options: &CffiHandleTableOptions,
) -> baml_outbound_result::Result {
    let is_panic = is_panic_value(&value);
    match external_to_outbound(&value, options) {
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
        Err(e) => sdk_panic_arm(format!("failed to encode thrown value: {e}"), options),
    }
}

/// Translate the engine's `Result<BexExternalValue, RuntimeError>` into the
/// `BamlOutboundResult` envelope. The only genuinely new logic is the
/// namespace check (error vs panic) and synthesizing infra classes for the
/// host-originated failures that never entered the VM as throws; value
/// materialization is reused verbatim via [`external_to_outbound`].
pub fn result_to_outbound(
    result: Result<BexExternalValue, RuntimeError>,
    options: &CffiHandleTableOptions,
) -> BamlOutboundResult {
    let inner = match result {
        Ok(value) => match external_to_outbound(&value, options) {
            Ok(ob) => baml_outbound_result::Result::Ok(ob),
            Err(e) => sdk_panic_arm(format!("failed to encode return value: {e}"), options),
        },

        // Clean `baml.sys.exit(code)` — the engine pulled the code out before
        // it could reach `UnhandledThrow`. Synthesize the `baml.panics.Exit`
        // value and set the exit discriminator so the host exits the process.
        Err(RuntimeError::Engine(EngineError::Exit { code })) => {
            let value = one_field_instance(EXIT_CLASS, "code", BexExternalValue::Int(code));
            match external_to_outbound(&value, options) {
                Ok(ob) => baml_outbound_result::Result::Panic(BamlOutboundPanic {
                    value: Some(ob),
                    trace: Vec::new(),
                    is_exit_panic: true,
                    exit_code: code,
                }),
                Err(e) => sdk_panic_arm(format!("failed to encode exit value: {e}"), options),
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
            thrown_arm(*value, lines, options)
        }

        // 🟥 A value/type mismatch at the call boundary is a *caller* type error,
        // not an SDK panic — route it to a structured `baml.errors.TypeMismatch`
        // (an `error` arm) so each host SDK can surface it as its native
        // type-error (Python `TypeError`) rather than an opaque `SdkPanic`. This
        // covers inbound-generics Gate-A failures (a `TypeVar` with no inference
        // evidence that must be specified, conflicting variance occurrences) and
        // ordinary argument-conversion mismatches alike.
        Err(RuntimeError::Engine(EngineError::TypeMismatch { message })) => {
            infra_error_arm(message_instance(TYPE_MISMATCH_CLASS, message), options)
        }

        // Every other 🟥 engine/VM-internal failure → one opaque `SdkPanic`,
        // its `Display` (incl. any formatted VM trace) carried as `message`.
        Err(RuntimeError::Engine(engine_err)) => sdk_panic_arm(engine_err.to_string(), options),

        // 🟥 `RuntimeError` direct arms → fine-grained `baml.errors.*`.
        Err(RuntimeError::Other(s)) => {
            infra_error_arm(message_instance(GENERIC_SDK_ERROR_CLASS, s), options)
        }
        Err(err @ RuntimeError::InvalidArgument { .. }) => infra_error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            options,
        ),
        Err(RuntimeError::Compilation { message }) => {
            infra_error_arm(message_instance(COMPILATION_ERROR_CLASS, message), options)
        }
        Err(RuntimeError::Access(inner)) => infra_error_arm(
            message_instance(ACCESS_ERROR_CLASS, inner.to_string()),
            options,
        ),
    };

    BamlOutboundResult {
        result: Some(inner),
    }
}

pub fn unhandled_spawn_error_to_outbound(error: UnhandledSpawnError) -> Vec<u8> {
    let options = CffiHandleTableOptions::for_in_process();
    let trace = bridge_ctypes::format_traceback_lines(error.trace.iter().map(|frame| {
        (
            frame.file_path.as_str(),
            frame.error_line,
            frame.function_name.as_str(),
        )
    }));
    BamlOutboundResult {
        result: Some(thrown_arm(error.value, trace, &options)),
    }
    .encode_to_vec()
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
    let options = CffiHandleTableOptions::for_in_process();
    let inner = match err {
        // Reuse the engine-error mapping verbatim for the wrapped RuntimeError.
        BridgeError::Runtime(rt) => result_to_outbound(Err(rt), &options)
            .result
            .expect("result_to_outbound always sets the result oneof"),

        // Bad function name / arguments → InvalidArgument.
        err @ (BridgeError::Ctypes(_)
        | BridgeError::MissingCallTarget
        | BridgeError::FunctionHandleTypeArgs
        | BridgeError::FunctionNotFound { .. }
        | BridgeError::MissingArgument { .. }
        | BridgeError::InvalidCallId) => infra_error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            &options,
        ),

        // Setup / internal host failures (NotInitialized, ProjectNotInitialized,
        // LockPoisoned, NotImplemented, DuplicateCallId, Internal) → GenericSdkError.
        err => infra_error_arm(
            message_instance(GENERIC_SDK_ERROR_CLASS, err.to_string()),
            &options,
        ),
    };

    BamlOutboundResult {
        result: Some(inner),
    }
    .encode_to_vec()
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
    let options = CffiHandleTableOptions::for_in_process();
    BamlOutboundResult {
        result: Some(sdk_panic_arm(panic_message(panic_info), &options)),
    }
    .encode_to_vec()
}

/// Call a BAML function and encode the result as `BamlOutboundResult` bytes.
///
/// The `catch_unwind` boundary wraps the engine call so a Rust panic surfaces
/// as a `baml.panics.SdkPanic` ⇒ `BamlOutboundPanic` (a catchable `BamlPanic`
/// in Python), not an opaque ABI panic. A panic during *encoding* (outside the
/// inner `catch_unwind` but still rare) escapes this function; the C-ABI entry
/// point keeps its own outer `catch_unwind` for that (encoding via
/// [`panic_to_outbound`]), and the PyO3 glue lets it become pyo3's
/// `PanicException`.
pub async fn call_and_encode(
    runtime: Arc<dyn Bex>,
    function_name: String,
    args: BexArgs,
    call_ctx: FunctionCallContext,
) -> Vec<u8> {
    let options = CffiHandleTableOptions::for_in_process();
    let _route = crate::register_active_call_runtime(call_ctx.host_call_id.0, &runtime);

    let caught = AssertUnwindSafe(runtime.call_function(&function_name, args, call_ctx))
        .catch_unwind()
        .await;

    let result = match caught {
        Ok(call_result) => result_to_outbound(call_result, &options),
        Err(panic_info) => BamlOutboundResult {
            result: Some(sdk_panic_arm(panic_message(panic_info.as_ref()), &options)),
        },
    };

    result.encode_to_vec()
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

/// Invoke an engine-owned callable referenced by an ordinary handle-table key
/// and encode the result through the same envelope path as a named call.
pub async fn call_handle_and_encode(
    runtime: Arc<dyn Bex>,
    handle_key: u64,
    BexArgs { required, optional }: BexArgs,
    call_ctx: FunctionCallContext,
) -> Vec<u8> {
    let mut supplied = required;
    supplied.extend(optional);
    let (handle, args) = match HANDLE_TABLE.resolve(handle_key) {
        Some(entry) => match &*entry {
            CffiHandleTableEntry::Adt(bex_project::BexExternalAdt::TaggedHeapHandle {
                ty: bex_project::RuntimeTy::Function { params, .. },
                heap_handle,
            }) => {
                let params = params.iter().enumerate().map(|(index, parameter)| {
                    (
                        parameter
                            .name
                            .as_ref()
                            .map_or_else(|| format!("arg{index}"), ToString::to_string),
                        parameter.is_required(),
                    )
                });
                let args = match partition_callable_args(handle_key, params, supplied) {
                    Ok(args) => args,
                    Err(err) => return error_to_outbound(err),
                };
                (heap_handle.clone(), args)
            }
            _ => {
                return error_to_outbound(BridgeError::Internal(
                    "handle does not reference a BAML callable".to_string(),
                ));
            }
        },
        None => {
            return error_to_outbound(BridgeError::Internal(
                "callable handle is no longer live".to_string(),
            ));
        }
    };

    let options = CffiHandleTableOptions::for_in_process();
    let _route = crate::register_active_call_runtime(call_ctx.host_call_id.0, &runtime);
    let caught = AssertUnwindSafe(runtime.call_callable(handle, args, call_ctx))
        .catch_unwind()
        .await;
    let result = match caught {
        Ok(call_result) => result_to_outbound(call_result, &options),
        Err(panic_info) => BamlOutboundResult {
            result: Some(sdk_panic_arm(panic_message(panic_info.as_ref()), &options)),
        },
    };
    result.encode_to_vec()
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
