//! Error types for bridge_cffi.

use thiserror::Error;

/// Errors that can occur during bridge operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error(transparent)]
    Ctypes(#[from] bridge_ctypes::CtypesError),
    #[error("Engine not initialized. Call create_baml_runtime first.")]
    NotInitialized,

    #[error("Project not initialized")]
    ProjectNotInitialized,

    #[error("Engine lock poisoned")]
    LockPoisoned,

    #[error("{0}")]
    Runtime(#[from] bex_project::RuntimeError),

    #[error("CallFunctionArgs.call_target must be set")]
    MissingCallTarget,

    #[error("type arguments are not supported when invoking a BAML function handle")]
    FunctionHandleTypeArgs,

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("Missing argument '{parameter}' for function '{function}'")]
    MissingArgument { function: String, parameter: String },

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("call_id {0} is already in use by an active call")]
    DuplicateCallId(u64),

    #[error("call_id must be a nonzero uint64")]
    InvalidCallId,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("{0}")]
    Startup(String),
}
