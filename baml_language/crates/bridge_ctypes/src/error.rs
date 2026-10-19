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

    /// The boundary carries string keys only; the kind that arrived is named.
    #[error("map keys cross the boundary as strings; got an {0} key")]
    MapKeyNotString(&'static str),

    #[error("key `{0}` occurs more than once")]
    DuplicateKey(String),

    #[error("{field} is not supported and must be empty")]
    UnsupportedTypeField { field: &'static str },

    #[error("handle key {0} occurs as more than one kind of handle")]
    HandleKindConflict(u64),

    /// Carries only the input length, not the input itself — untrusted hex
    /// blobs can be up to the FFI decode cap (~67M chars), and embedding
    /// them in error messages bloats logs and exposes payload contents.
    #[error("Invalid bigint hex string ({len} bytes)")]
    InvalidBigint { len: usize },

    /// Carries only the input length for over-cap decimal type literals, so a
    /// hostile descriptor cannot amplify logs by echoing its full payload.
    #[error("Invalid decimal bigint literal ({len} bytes)")]
    InvalidBigintLiteral { len: usize },

    /// A list, map, class or callable node arrived without the type it has.
    #[error("InboundValue.{node} must state its type in value_type ({expected})")]
    MissingValueType {
        node: &'static str,
        expected: &'static str,
    },

    /// A node whose type the wire implies arrived with a `value_type`.
    #[error("InboundValue.{node} implies its own type; value_type must be absent")]
    UnexpectedValueType { node: &'static str },

    /// The stated type is not the shape of the node it is on. `stated` is
    /// rendered bounded: a type is the host's text.
    #[error("InboundValue.{node} states `{stated}`, which is not {expected}")]
    ValueTypeShape {
        node: &'static str,
        expected: &'static str,
        stated: String,
    },

    /// A stated type mentions a type variable, so it determines no value.
    #[error(
        "InboundValue.value_type `{stated}` is not realized: it mentions the type variable `{var}`"
    )]
    ValueTypeNotRealized { stated: String, var: String },

    /// A wire type lacks a part no type can do without: its variant, a
    /// container's inner type, a literal's value, a kind.
    #[error("wire type is missing its {position}")]
    IncompleteType { position: &'static str },

    #[error("Union selected type `{selected}` is not a member of declared union `{union}`")]
    UnionSelectedTypeNotMember { selected: String, union: String },

    /// The `js_number_value` arm asked the engine to read a JavaScript number
    /// by the type of its slot. A host states `int_value` or `float_value`
    /// itself; the field stays on the wire only until the generated protobuf
    /// code is next regenerated.
    #[error("InboundValue.js_number_value is no longer accepted: send int_value or float_value")]
    JsNumberValueRetired,

    #[error("Internal error: {0}")]
    InternalError(String),
}
