//! Trait for extracting concrete types from BamlValue (owned).
//!
//! Full implementations will be added in Phase 2+.

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
