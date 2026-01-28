//! Error types for baml_bridge_cffi.

use thiserror::Error;

/// Errors that can occur during bridge operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("Engine not initialized. Call create_baml_runtime first.")]
    NotInitialized,

    #[error("Project not initialized")]
    ProjectNotInitialized,

    #[error("Engine lock poisoned")]
    LockPoisoned,

    #[error("Compilation error: {message}")]
    Compilation { message: String },

    #[error("Engine error: {0}")]
    Engine(#[from] bex_engine::EngineError),

    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("Null buffer pointer")]
    NullBuffer,

    #[error("Null function name pointer")]
    NullFunctionName,

    #[error("Invalid UTF-8 in function name: {0}")]
    InvalidFunctionName(#[from] std::str::Utf8Error),

    #[error("Handle values not supported")]
    HandleNotSupported,

    #[error("Map entry missing key")]
    MapEntryMissingKey,

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("Missing argument '{parameter}' for function '{function}'")]
    MissingArgument { function: String, parameter: String },

    #[error("Not implemented: {0}")]
    NotImplemented(String),
}
