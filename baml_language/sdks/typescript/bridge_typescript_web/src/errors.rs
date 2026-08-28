//! JavaScript boundary-error adapters.

use bridge_cffi::handle::HandleError;
use wasm_bindgen::{JsError, JsValue};

pub(crate) const INVALID_ARGUMENT: &str = "invalid_argument";
pub(crate) const NOT_INITIALIZED: &str = "not_initialized";
pub(crate) const COMPILATION: &str = "compilation";
pub(crate) const CLIENT: &str = "client";

/// Build an Error-like object whose stable `code` survives the wasm-bindgen
/// exception boundary without relying on formatted Rust or N-API prefixes.
pub(crate) fn setup_error(code: &'static str, message: impl AsRef<str>) -> JsValue {
    let error = js_sys::Error::new(message.as_ref());
    error.set_name("BamlBridgeSetupError");
    let _ = js_sys::Reflect::set(
        error.as_ref(),
        &JsValue::from_str("code"),
        &JsValue::from_str(code),
    );
    error.into()
}

pub(crate) fn bridge_error(error: &bridge_cffi::BridgeError) -> JsValue {
    use bex_project::RuntimeError;
    use bridge_cffi::BridgeError;

    let code = match &error {
        BridgeError::NotInitialized | BridgeError::ProjectNotInitialized => NOT_INITIALIZED,
        BridgeError::Ctypes(_)
        | BridgeError::MissingCallTarget
        | BridgeError::FunctionHandleTypeArgs
        | BridgeError::InvalidFunctionOperation(_)
        | BridgeError::MissingArgument { .. }
        | BridgeError::InvalidCallId => INVALID_ARGUMENT,
        BridgeError::Runtime(RuntimeError::InvalidArgument { .. }) => INVALID_ARGUMENT,
        BridgeError::Runtime(RuntimeError::Compilation { .. }) => COMPILATION,
        BridgeError::Runtime(
            RuntimeError::Other(_) | RuntimeError::Engine(_) | RuntimeError::Access(_),
        )
        | BridgeError::LockPoisoned
        | BridgeError::FunctionNotFound { .. }
        | BridgeError::NotImplemented(_)
        | BridgeError::DuplicateCallId(_)
        | BridgeError::Internal(_)
        | BridgeError::Startup(_) => CLIENT,
    };
    setup_error(code, error.to_string())
}

pub(crate) fn handle_error(operation: &'static str, error: &HandleError) -> JsError {
    JsError::new(&format!("{operation}: {error}"))
}

pub(crate) fn unexpected_handle_type(
    operation: &'static str,
    expected: i32,
    actual: i32,
) -> JsError {
    JsError::new(&format!(
        "{operation}: shared handle type mismatch (expected {expected}, got {actual})"
    ))
}
