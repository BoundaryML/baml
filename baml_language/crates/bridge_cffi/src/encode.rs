//! Shared `Result<BexExternalValue, RuntimeError>` → `BamlOutboundResult`
//! translation (31e-phase4).
//!
//! The whole classify-and-encode lives here, in the bridge — not in bex.
//! Both the C-ABI shim (`ffi/functions.rs`) and the PyO3 path
//! (`bridge_python`'s `runtime.rs`) call [`call_and_encode`], so the
//! `catch_unwind` → `SdkPanic` boundary and the error/panic routing are
//! defined exactly once.
//!
//! Routing recovers the panic-vs-error distinction the same way the VM does
//! internally: by namespace. A thrown `BexExternalValue::Instance` whose
//! `class_name` is under `baml.panics.*` is a panic; anything else is an
//! error. Host-originated infra failures that never entered the VM as throws
//! are *synthesized* into the `baml.errors.*` / `baml.panics.SdkPanic` classes
//! added in 31d-phase3.

use std::{panic::AssertUnwindSafe, sync::Arc};

use bex_project::{Bex, BexArgs, BexExternalValue, EngineError, FunctionCallContext, RuntimeError};
use bridge_ctypes::{
    CffiHandleTableOptions,
    baml_core::cffi::{
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
const SDK_PANIC_CLASS: &str = "baml.panics.SdkPanic";
const EXIT_CLASS: &str = "baml.panics.Exit";

/// Build a one-field class instance (`class_name { field: value }`).
fn one_field_instance(class_name: &str, field: &str, value: BexExternalValue) -> BexExternalValue {
    let mut fields = IndexMap::new();
    fields.insert(field.to_string(), value);
    BexExternalValue::Instance {
        class_name: class_name.to_string(),
        fields,
    }
}

/// Build a `class_name { message: <message> }` instance — the shape of every
/// synthesized `baml.errors.*` / `baml.panics.SdkPanic` infra class.
fn message_instance(class_name: &str, message: String) -> BexExternalValue {
    one_field_instance(class_name, "message", BexExternalValue::String(message))
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
fn error_arm(
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
            let lines = bex_vm::format_traceback_lines(
                trace
                    .iter()
                    .map(|f| (f.file_path.as_str(), f.error_line, f.function_name.as_str())),
            );
            thrown_arm(*value, lines, options)
        }

        // Every other 🟥 engine/VM-internal failure → one opaque `SdkPanic`,
        // its `Display` (incl. any formatted VM trace) carried as `message`.
        Err(RuntimeError::Engine(engine_err)) => sdk_panic_arm(engine_err.to_string(), options),

        // 🟥 `RuntimeError` direct arms → fine-grained `baml.errors.*`.
        Err(RuntimeError::Other(s)) => {
            error_arm(message_instance(GENERIC_SDK_ERROR_CLASS, s), options)
        }
        Err(err @ RuntimeError::InvalidArgument { .. }) => error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            options,
        ),
        Err(RuntimeError::Compilation { message }) => {
            error_arm(message_instance(COMPILATION_ERROR_CLASS, message), options)
        }
        Err(RuntimeError::Access(inner)) => error_arm(
            message_instance(ACCESS_ERROR_CLASS, inner.to_string()),
            options,
        ),
    };

    BamlOutboundResult {
        result: Some(inner),
    }
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
        err @ (BridgeError::NullFunctionName
        | BridgeError::InvalidFunctionName(_)
        | BridgeError::FunctionNotFound { .. }
        | BridgeError::MissingArgument { .. }) => error_arm(
            message_instance(INVALID_ARGUMENT_CLASS, err.to_string()),
            &options,
        ),

        // Setup / internal host failures (Ctypes, NotInitialized,
        // ProjectNotInitialized, LockPoisoned, NotImplemented, DuplicateCallId,
        // Internal) → GenericSdkError.
        err => error_arm(
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

/// Call a BAML function and encode the result as `BamlOutboundResult` bytes.
///
/// The `catch_unwind` boundary wraps the engine call so a Rust panic surfaces
/// as a `baml.panics.SdkPanic` ⇒ `BamlOutboundPanic` (a catchable `BamlPanic`
/// in Python), not an opaque ABI panic. A panic during *encoding* (outside the
/// inner `catch_unwind` but still rare) escapes this function; the C-ABI shim
/// keeps its own outer `catch_unwind` for that, and the PyO3 glue lets it
/// become pyo3's `PanicException`.
pub async fn call_and_encode(
    runtime: Arc<dyn Bex>,
    function_name: String,
    args: BexArgs,
    call_ctx: FunctionCallContext,
) -> Vec<u8> {
    let options = CffiHandleTableOptions::for_in_process();

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
