//! The neutral value model (TASK/baml-query-scope.md §5.5).
//!
//! The core owns this small enum; providers decode their storage codecs
//! into it. Semantics (navigation, comparison, rendering) live in
//! `semantics`; nothing here depends on any storage crate.

/// Field presence within a class value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// The field carries a value (possibly `Value::Null` for typed nulls).
    Present,
    /// The field was captured as null.
    Null,
    /// The field was absent from the capture.
    Absent,
}

/// One captured BAML value, decoded from provider storage.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    /// Arbitrary-precision integer, minimal decimal rendering.
    BigInt(String),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    /// Key/value entries; comparison is order-insensitive (key-sorted).
    Map(Vec<(String, Value)>),
    Class {
        name: String,
        fields: Vec<(String, Presence, Option<Value>)>,
    },
    Enum {
        name: String,
        variant: String,
    },
    Media {
        kind: String,
        mime: String,
        content: MediaContent,
    },
    /// A subtree elided at capture time: unavailability, not data.
    Omitted {
        reason: String,
    },
}

/// Media payloads: inline bytes or an external reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaContent {
    Bytes(Vec<u8>),
    Url(String),
}
