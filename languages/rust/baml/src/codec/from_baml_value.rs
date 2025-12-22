//! Trait for extracting concrete types from BamlValue (owned).

use crate::error::BamlError;

use super::baml_value::BamlValue;
use super::known_types::KnownTypes;

/// Trait for extracting concrete types from BamlValue.
///
/// Primitives are implemented in the baml crate.
/// Known types are implemented by generated code.
pub trait FromBamlValue<T: KnownTypes, S: KnownTypes>: Sized {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError>;
}

// =============================================================================
// Primitive FromBamlValue implementations
// =============================================================================

impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for String {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::String(s) => Ok(s),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for i64 {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Int(i) => Ok(i),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for f64 {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Float(f) => Ok(f),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for bool {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Bool(b) => Ok(b),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for () {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Null => Ok(()),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}
