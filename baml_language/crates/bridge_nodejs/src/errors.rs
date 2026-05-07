//! Error converters for bridge_nodejs — mirrors bridge_python/src/errors.rs.

use napi::Status;

pub fn bridge_error_to_napi(err: bridge_cffi::error::BridgeError) -> napi::Error {
    use bridge_cffi::BridgeError;
    match err {
        BridgeError::Ctypes(e) => napi::Error::new(
            Status::InvalidArg,
            format!("BamlError: BamlInvalidArgumentError: Ctypes error: {e}"),
        ),
        // FIXME: Uses Status::InvalidArg and references "create_baml_runtime" (a C-API function
        // not exposed by the Node bridge). Legacy engine/ had no NotInitialized error (runtime
        // always constructed at init). bridge_python has the same message/classification.
        // Leaving as-is: this code path is unreachable via BamlRuntime (uses self.bex directly,
        // not global RUNTIME_INSTANCE). Fix in bridge_cffi if it ever becomes reachable.
        BridgeError::NotInitialized => napi::Error::new(
            Status::InvalidArg,
            "BamlError: BamlInvalidArgumentError: Engine not initialized. Call create_baml_runtime first.",
        ),
        BridgeError::ProjectNotInitialized => napi::Error::new(
            Status::GenericFailure,
            "BamlError: BamlClientError: Project not initialized",
        ),
        BridgeError::LockPoisoned => napi::Error::new(
            Status::GenericFailure,
            "BamlError: BamlClientError: Internal error: lock poisoned",
        ),
        BridgeError::Runtime(runtime_error) => runtime_error_to_napi(runtime_error),
        BridgeError::NullFunctionName => napi::Error::new(
            Status::InvalidArg,
            "BamlError: BamlInvalidArgumentError: Null function name pointer",
        ),
        BridgeError::InvalidFunctionName(e) => napi::Error::new(
            Status::InvalidArg,
            format!("BamlError: BamlInvalidArgumentError: Invalid UTF-8 in function name: {e}"),
        ),
        BridgeError::FunctionNotFound { name } => napi::Error::new(
            Status::InvalidArg,
            format!("BamlError: BamlInvalidArgumentError: Function not found: {name}"),
        ),
        BridgeError::MissingArgument {
            function,
            parameter,
        } => napi::Error::new(
            Status::InvalidArg,
            format!(
                "BamlError: BamlInvalidArgumentError: Missing argument '{parameter}' for function '{function}'"
            ),
        ),
        BridgeError::NotImplemented(msg) => napi::Error::new(
            Status::InvalidArg,
            format!("BamlError: BamlInvalidArgumentError: Not implemented: {msg}"),
        ),
        BridgeError::DuplicateCallId(id) => napi::Error::new(
            Status::InvalidArg,
            format!(
                "BamlError: BamlInvalidArgumentError: call_id {id} is already in use by an active call"
            ),
        ),
        BridgeError::Internal(msg) => napi::Error::new(
            Status::GenericFailure,
            format!("BamlError: BamlClientError: Internal error: {msg}"),
        ),
    }
}

pub fn runtime_error_to_napi(err: bex_project::RuntimeError) -> napi::Error {
    use bex_project::RuntimeError;
    match &err {
        RuntimeError::InvalidArgument { .. } => napi::Error::new(
            Status::InvalidArg,
            format!("BamlError: BamlInvalidArgumentError: {err}"),
        ),
        RuntimeError::Engine(engine_err) => {
            use bex_project::EngineError;
            match engine_err {
                EngineError::FunctionNotFound { .. } => napi::Error::new(
                    Status::InvalidArg,
                    format!("BamlError: BamlInvalidArgumentError: {err}"),
                ),
                e if bex_project::is_cancelled_engine_error(e) => napi::Error::new(
                    Status::Cancelled,
                    format!("BamlError: BamlCancelledError: {err}"),
                ),
                _ => napi::Error::new(
                    Status::GenericFailure,
                    format!("BamlError: BamlClientError: {err}"),
                ),
            }
        }
        _ => napi::Error::new(
            Status::GenericFailure,
            format!("BamlError: BamlClientError: {err}"),
        ),
    }
}

pub fn invalid_argument_error(message: impl Into<String>) -> napi::Error {
    napi::Error::new(
        Status::InvalidArg,
        format!("BamlError: BamlInvalidArgumentError: {}", message.into()),
    )
}
