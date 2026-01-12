//! Trait for zero-copy borrowing from `BamlValue`.

use super::{baml_value::BamlValue, known_types::KnownTypes};
use crate::error::BamlError;

/// Trait for zero-copy borrowing from `BamlValue`.
///
/// Use for primitives (&str, i64, f64, bool) and references to known types.
/// Note: Does NOT work for converted containers (use `get()` for Vec<Person>).
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

// =============================================================================
// Container FromBamlValueRef implementations (raw container refs only)
// =============================================================================

use std::collections::HashMap;

/// Raw list ref - returns slice of `BamlValue`, NOT converted types
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for &'a [BamlValue<T, S>] {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::List(l) => Ok(l.as_slice()),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}

/// Raw map ref - returns ref to `HashMap` of `BamlValue`, NOT converted values
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S>
    for &'a HashMap<String, BamlValue<T, S>>
{
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Map(m) => Ok(m),
            other => Err(BamlError::type_check::<Self>(other)),
        }
    }
}

// =============================================================================
// Dynamic Type FromBamlValueRef implementations
// =============================================================================

use super::dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion};

/// `DynamicClass` ref
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for &'a DynamicClass<T, S> {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicClass(dc) => Ok(dc),
            other => Err(BamlError::type_check::<DynamicClass<T, S>>(other)),
        }
    }
}

/// `DynamicEnum` ref
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for &'a DynamicEnum {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicEnum(de) => Ok(de),
            other => Err(BamlError::type_check::<DynamicEnum>(other)),
        }
    }
}

/// `DynamicUnion` ref
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S> for &'a DynamicUnion<T, S> {
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicUnion(du) => Ok(du),
            other => Err(BamlError::type_check::<DynamicUnion<T, S>>(other)),
        }
    }
}

// =============================================================================
// Wrapper type FromBamlValueRef implementations
// =============================================================================

use crate::{
    error::Unknown,
    types::{Checked, StreamState},
};

/// Checked ref - returns reference to the Checked wrapper containing
/// `BamlValue`
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S>
    for &'a Checked<Box<BamlValue<T, S>>>
{
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Checked(c) => Ok(c),
            other => Err(BamlError::type_check::<Checked<Unknown>>(other)),
        }
    }
}

/// `StreamState` ref - returns reference to the `StreamState` wrapper
/// containing `BamlValue`
impl<'a, T: KnownTypes, S: KnownTypes> FromBamlValueRef<'a, T, S>
    for &'a StreamState<Box<BamlValue<T, S>>>
{
    fn from_baml_value_ref(value: &'a BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::StreamState(ss) => Ok(ss),
            other => Err(BamlError::type_check::<StreamState<Unknown>>(other)),
        }
    }
}
