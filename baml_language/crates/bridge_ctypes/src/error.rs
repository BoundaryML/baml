//! Error types used by the shared ctypes conversion logic.

use thiserror::Error;

/// Errors that can occur during value encoding/decoding for the bridge.
#[derive(Debug, Error)]
pub enum CtypesError {
    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("Null buffer pointer")]
    NullBuffer,

    #[error("Invalid handle key: {0}")]
    InvalidHandleKey(u64),

    #[error("Map entry missing key")]
    MapEntryMissingKey,

    #[error("Internal error: {0}")]
    InternalError(String),
}
