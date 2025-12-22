//! Trait for zero-copy borrowing from BamlValue.

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

// =============================================================================
// Primitive FromBamlValueRef implementations
// =============================================================================

impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for &'a str {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::String(s) => Ok(s.as_str()),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}

// Copy types return by value (not reference)
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for i64 {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Int(i) => Ok(*i),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}

impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for f64 {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Float(f) => Ok(*f),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}

impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for bool {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Bool(b) => Ok(*b),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}
