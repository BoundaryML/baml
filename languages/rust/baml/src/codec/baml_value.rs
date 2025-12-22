//! BamlValue - a dynamically-typed BAML value.

use std::collections::HashMap;

use crate::error::{BamlTypeName, FullTypeName};
use crate::types::{Checked, StreamState};

use super::dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion};
use super::known_types::KnownTypes;

/// A dynamically-typed BAML value, parameterized by two type enums:
/// - `T`: Regular known types (e.g., `types::Person`)
/// - `S`: Stream known types (e.g., `stream_types::Person` with Option fields)
///
/// In non-streaming contexts, `StreamKnown(S)` is an invariant (never appears).
#[derive(Debug, Clone)]
pub enum BamlValue<T: KnownTypes, S: KnownTypes> {
    // Primitives
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    List(Vec<BamlValue<T, S>>),
    Map(HashMap<String, BamlValue<T, S>>),

    // Project-specific known types
    Known(T),        // Regular types (complete)
    StreamKnown(S),  // Stream types (partial) - invariant in non-streaming

    // Wrappers (contain BamlValue recursively)
    Checked(Checked<Box<BamlValue<T, S>>>),
    StreamState(StreamState<Box<BamlValue<T, S>>>),

    // Fallback for truly unknown types (e.g., from TypeBuilder at runtime)
    DynamicClass(DynamicClass<T, S>),
    DynamicEnum(DynamicEnum),
    DynamicUnion(DynamicUnion<T, S>),
}

/// Implement FullTypeName for BamlValue so it can be used with BamlError::type_check
impl<T: KnownTypes, S: KnownTypes> FullTypeName for BamlValue<T, S> {
    /// Get the full type name for error messages.
    /// Returns descriptive names like:
    /// - Primitives: "String", "Int", "Float", "Bool", "Null"
    /// - Containers: "List<?>", "Map<String, ?>" (element types unknown at runtime)
    /// - Wrappers: "Checked<?>", "StreamState<?>" (inner type requires recursion)
    /// - Dynamic: "DynamicClass(PersonInfo)", "DynamicEnum(Sentiment)", "DynamicUnion(FooOrBar)"
    fn full_type_name(&self) -> String {
        match self {
            BamlValue::String(_) => String::baml_type_name(),
            BamlValue::Int(_) => i64::baml_type_name(),
            BamlValue::Float(_) => f64::baml_type_name(),
            BamlValue::Bool(_) => bool::baml_type_name(),
            BamlValue::Null => <()>::baml_type_name(),
            BamlValue::List(_) => "List<?>".to_string(), // Can't know element type at runtime
            BamlValue::Map(_) => "Map<String, ?>".to_string(),
            BamlValue::Known(t) => t.type_name().to_string(),
            BamlValue::StreamKnown(s) => s.type_name().to_string(),
            BamlValue::Checked(c) => format!("Checked<{}>", c.value.full_type_name()),
            BamlValue::StreamState(ss) => format!("StreamState<{}>", ss.value.full_type_name()),
            BamlValue::DynamicClass(dc) => dc.full_type_name(),
            BamlValue::DynamicEnum(de) => de.full_type_name(),
            BamlValue::DynamicUnion(du) => du.full_type_name(),
        }
    }
}
