//! Trait for zero-copy borrowing from BamlValue.
//!
//! Full implementations will be added in Phase 2+.

use crate::error::BamlError;

use super::baml_value::BamlValue;
use super::known_types::KnownTypes;

/// Trait for zero-copy borrowing from BamlValue.
///
/// Use for primitives (&str, i64, f64, bool) and references to known types.
/// Note: Does NOT work for converted containers (use get() for Vec<Person>).
pub trait FromBamlValueRef<'a, T: KnownTypes, S: KnownTypes>: Sized {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError>;
}
