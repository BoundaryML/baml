//! Error types for `bex_factory`.

use thiserror::Error;

/// Errors that can occur during runtime operations.
#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Invalid argument: {name}")]
    InvalidArgument { name: String },

    #[error("Compilation error: {message}")]
    Compilation { message: String },

    #[error("{0}")]
    Engine(#[from] bex_engine::EngineError),

    #[error("Failed to convert result to owned value: {0}")]
    Access(#[from] bex_heap::AccessError),
}
