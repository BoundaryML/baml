//! Trait for extracting concrete types from `BamlValue` (owned).

use super::{baml_value::BamlValue, known_types::KnownTypes};
use crate::error::BamlError;

/// Trait for extracting concrete types from `BamlValue`.
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

// =============================================================================
// Identity and Dynamic Type FromBamlValue implementations
// =============================================================================

use super::dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion};

/// Identity impl - extract `BamlValue` from `BamlValue`
impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for BamlValue<T, S> {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        Ok(value)
    }
}

/// `DynamicClass` extraction
impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for DynamicClass<T, S> {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicClass(dc) => Ok(dc),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

/// `DynamicEnum` extraction
impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for DynamicEnum {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicEnum(de) => Ok(de),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

/// `DynamicUnion` extraction
impl<T: KnownTypes, S: KnownTypes> FromBamlValue<T, S> for DynamicUnion<T, S> {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::DynamicUnion(du) => Ok(du),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

// =============================================================================
// Container FromBamlValue blanket implementations
// =============================================================================

use std::collections::HashMap;

use crate::error::BamlTypeName;

/// Option<V> - handles nullable fields (string?, int?, etc.)
/// Note: `BamlTypeName` bound is for consistency with Vec/HashMap, even though
/// Option never produces type errors itself (it delegates to V or returns None
/// for Null).
impl<T: KnownTypes, S: KnownTypes, V: FromBamlValue<T, S> + BamlTypeName> FromBamlValue<T, S>
    for Option<V>
{
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Null => Ok(None),
            other => Ok(Some(V::from_baml_value(other)?)),
        }
    }
}

/// Vec<V> - handles list types (string[], Person[], etc.)
impl<T: KnownTypes, S: KnownTypes, V: FromBamlValue<T, S> + BamlTypeName> FromBamlValue<T, S>
    for Vec<V>
{
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::List(list) => list.into_iter().map(V::from_baml_value).collect(),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

/// `HashMap`<String, V> - handles map types (map<string, int>, etc.)
impl<T: KnownTypes, S: KnownTypes, V: FromBamlValue<T, S> + BamlTypeName> FromBamlValue<T, S>
    for HashMap<String, V>
{
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Map(map) => map
                .into_iter()
                .map(|(k, v)| Ok((k, V::from_baml_value(v)?)))
                .collect(),
            other => Err(BamlError::type_check::<Self>(&other)),
        }
    }
}

// =============================================================================
// Wrapper type blanket implementations
// =============================================================================

use crate::types::{Checked, StreamState, StreamingState};

/// Checked<V> blanket impl - works for any V: `FromBamlValue`
impl<T: KnownTypes, S: KnownTypes, V: FromBamlValue<T, S>> FromBamlValue<T, S> for Checked<V> {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::Checked(checked) => {
                // Extract inner BamlValue, convert to V
                let inner = V::from_baml_value(*checked.value)?;
                Ok(Checked {
                    value: inner,
                    checks: checked.checks,
                })
            }
            // Allow unwrapped value (no checks = empty checks)
            other => {
                let inner = V::from_baml_value(other)?;
                Ok(Checked {
                    value: inner,
                    checks: HashMap::new(),
                })
            }
        }
    }
}

/// `StreamState`<V> blanket impl - works for any V: `FromBamlValue`
impl<T: KnownTypes, S: KnownTypes, V: FromBamlValue<T, S>> FromBamlValue<T, S> for StreamState<V> {
    fn from_baml_value(value: BamlValue<T, S>) -> Result<Self, BamlError> {
        match value {
            BamlValue::StreamState(ss) => Ok(StreamState {
                value: V::from_baml_value(*ss.value)?,
                state: ss.state,
            }),
            // Treat unwrapped as Done
            other => Ok(StreamState {
                value: V::from_baml_value(other)?,
                state: StreamingState::Done,
            }),
        }
    }
}
