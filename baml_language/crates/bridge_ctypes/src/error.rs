//! Error types used by the shared ctypes conversion logic.

use thiserror::Error;

/// Errors that can occur during value encoding/decoding for the bridge.
#[derive(Debug, Error)]
pub enum CtypesError {
    #[error("Protobuf decode error: {}", bounded_decode_error(.0))]
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
    DuplicateKey(bex_project::Bounded),

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
        stated: bex_project::Bounded,
    },

    /// A wire type is realized: a type variable or an associated-type
    /// projection determines no value, so no wire position may hold one.
    #[error("a wire type may not contain a {variant}; every type on the wire is realized")]
    UnrealizedType { variant: &'static str },

    /// A wire type lacks a part no type can do without: its variant, a
    /// container's inner type, a literal's value, a kind.
    #[error("wire type is missing its {position}")]
    IncompleteType { position: &'static str },

    /// A stated type names a declaration by its display label. A label is
    /// never a reference: a runtime declaration is referenced by the row the
    /// host received it under.
    #[error(
        "wire type names `{label}` by label only; a runtime declaration is referenced by its handle"
    )]
    AnonymousHeadStated { label: bex_project::Bounded },

    /// A float literal type whose text is not a finite float. Carries only
    /// the input length: the text is the host's.
    #[error("Invalid float literal in a wire type ({len} bytes)")]
    InvalidFloatLiteral { len: usize },

    /// A `live` head's row is not a reference to an engine object, so it can
    /// name no declaration.
    #[error("wire type references handle key {0}, which is not a declaration")]
    LiveHeadNotADeclaration(u64),

    /// A `class_value` states a runtime declaration as its class. An instance
    /// of a runtime class crosses by reference, never by fields.
    #[error("class_value states a runtime class; an instance of one is passed as it was received")]
    ClassValueOfRuntimeClass,

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

/// prost renders a decode error as `failed to decode Protobuf message: <path>:
/// <reason>`, where the path names every enclosing field and so grows with the
/// nesting depth of a payload the host controls. The reason is kept whole; the
/// path is bounded like every other value a diagnostic quotes.
fn bounded_decode_error(error: &prost::DecodeError) -> String {
    let rendered = error.to_string();
    let rendered = rendered
        .strip_prefix("failed to decode Protobuf message: ")
        .unwrap_or(&rendered);
    match rendered.rsplit_once(": ") {
        Some((path, reason)) => format!("{reason} (at `{}`)", bex_project::Bounded::text(path)),
        None => rendered.to_string(),
    }
}
