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

    /// Carries only the input length for over-cap decimal type literals, so a
    /// hostile descriptor cannot amplify logs by echoing its full payload.
    #[error("Invalid decimal bigint literal ({len} bytes)")]
    InvalidBigintLiteral { len: usize },

    #[error(
        "Invalid InboundValue.value_type: a root union or optional does not identify one exact selected type"
    )]
    InvalidInboundValueTypeRootUnion,

    #[error("Union selected type `{selected}` is not a member of declared union `{union}`")]
    UnionSelectedTypeNotMember { selected: String, union: String },

    #[error("Internal error: {0}")]
    InternalError(String),
}
