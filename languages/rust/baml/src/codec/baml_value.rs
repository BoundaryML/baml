//! BamlValue - a dynamically-typed BAML value.
//!
//! This module will be implemented in Phase 1.

use std::collections::HashMap;

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
