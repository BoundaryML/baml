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

    /// Carries only the input length, not the input itself — untrusted hex
    /// blobs can be up to the FFI decode cap (~67M chars), and embedding
    /// them in error messages bloats logs and exposes payload contents.
    #[error("Invalid bigint hex string ({len} bytes)")]
    InvalidBigint { len: usize },

    #[error("Invalid inbound union metadata: {0}")]
    InvalidUnionMetadata(String),

    #[error("Invalid inbound collection metadata: {0}")]
    InvalidCollectionMetadata(String),

    #[error("Invalid {kind} literal type metadata")]
    InvalidLiteralMetadata { kind: &'static str },

    #[error("Internal error: {0}")]
    InternalError(String),
}
