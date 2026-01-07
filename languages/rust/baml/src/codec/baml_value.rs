//! `BamlValue` - a dynamically-typed BAML value.

use std::collections::HashMap;

use super::{
    dynamic_types::{DynamicClass, DynamicEnum, DynamicUnion},
    from_baml_value::FromBamlValue,
    from_baml_value_ref::FromBamlValueRef,
    known_types::KnownTypes,
};
use crate::{
    error::{BamlError, BamlTypeName, FullTypeName},
    types::{Checked, StreamState},
};

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
    Known(T),       // Regular types (complete)
    StreamKnown(S), // Stream types (partial) - invariant in non-streaming

    // Wrappers (contain BamlValue recursively)
    Checked(Checked<Box<BamlValue<T, S>>>),
    StreamState(StreamState<Box<BamlValue<T, S>>>),

    // Fallback for truly unknown types (e.g., from TypeBuilder at runtime)
    DynamicClass(DynamicClass<T, S>),
    DynamicEnum(DynamicEnum),
    DynamicUnion(DynamicUnion<T, S>),
}

/// Implement `FullTypeName` for `BamlValue` so it can be used with
/// `BamlError::type_check`
impl<T: KnownTypes, S: KnownTypes> FullTypeName for BamlValue<T, S> {
    /// Get the full type name for error messages.
    /// Returns descriptive names like:
    /// - Primitives: "String", "Int", "Float", "Bool", "Null"
    /// - Containers: "List<?>", "Map<String, ?>" (element types unknown at
    ///   runtime)
    /// - Wrappers: "Checked<?>", "StreamState<?>" (inner type requires
    ///   recursion)
    /// - Dynamic: "DynamicClass(PersonInfo)", "DynamicEnum(Sentiment)",
    ///   "DynamicUnion(FooOrBar)"
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

impl<T: KnownTypes, S: KnownTypes> BamlValue<T, S> {
    /// Convert this `BamlValue` to the specified type.
    pub fn get<V: FromBamlValue<T, S>>(self) -> Result<V, BamlError> {
        V::from_baml_value(self)
    }

    /// Borrow this `BamlValue` as the specified type (zero-copy).
    pub fn get_ref<'a, V: FromBamlValueRef<'a, T, S>>(&'a self) -> Result<V, BamlError> {
        V::from_baml_value_ref(self)
    }
}

// =============================================================================
// BamlEncode implementation for BamlValue
// =============================================================================

use super::traits::{BamlDecode, BamlEncode};
use crate::proto::baml_cffi_v1::{
    host_map_entry, host_value, HostClassValue, HostEnumValue, HostListValue, HostMapEntry,
    HostMapValue, HostValue,
};

impl<T: KnownTypes, S: KnownTypes> BamlEncode for BamlValue<T, S> {
    fn baml_encode(&self) -> HostValue {
        match self {
            BamlValue::Null => HostValue { value: None },
            BamlValue::String(s) => HostValue {
                value: Some(host_value::Value::StringValue(s.clone())),
            },
            BamlValue::Int(i) => HostValue {
                value: Some(host_value::Value::IntValue(*i)),
            },
            BamlValue::Float(f) => HostValue {
                value: Some(host_value::Value::FloatValue(*f)),
            },
            BamlValue::Bool(b) => HostValue {
                value: Some(host_value::Value::BoolValue(*b)),
            },
            BamlValue::List(items) => HostValue {
                value: Some(host_value::Value::ListValue(HostListValue {
                    values: items
                        .iter()
                        .map(super::traits::BamlEncode::baml_encode)
                        .collect(),
                })),
            },
            BamlValue::Map(map) => HostValue {
                value: Some(host_value::Value::MapValue(HostMapValue {
                    entries: map
                        .iter()
                        .map(|(k, v)| HostMapEntry {
                            key: Some(host_map_entry::Key::StringKey(k.clone())),
                            value: Some(v.baml_encode()),
                        })
                        .collect(),
                })),
            },
            BamlValue::DynamicClass(dc) => {
                let entries = dc
                    .fields()
                    .map(|(k, v)| HostMapEntry {
                        key: Some(host_map_entry::Key::StringKey(k.to_string())),
                        value: Some(v.baml_encode()),
                    })
                    .collect();
                HostValue {
                    value: Some(host_value::Value::ClassValue(HostClassValue {
                        name: dc.name().to_string(),
                        fields: entries,
                    })),
                }
            }
            BamlValue::DynamicEnum(de) => HostValue {
                value: Some(host_value::Value::EnumValue(HostEnumValue {
                    name: de.name().to_string(),
                    value: de.value.clone(),
                })),
            },
            BamlValue::DynamicUnion(du) => {
                // Encode the inner value - union variant is handled at a higher level
                du.value.baml_encode()
            }
            BamlValue::Known(_) => {
                // Known types should implement BamlEncode
                // For now, encode as null since we don't have access to the actual type
                // This is a fallback - users should encode known types directly
                HostValue { value: None }
            }
            BamlValue::StreamKnown(_) => {
                // Stream known types - encode as null for now
                HostValue { value: None }
            }
            BamlValue::Checked(c) => {
                // Encode the inner value (checks are metadata, not data)
                c.value.baml_encode()
            }
            BamlValue::StreamState(ss) => {
                // Encode the inner value
                ss.value.baml_encode()
            }
        }
    }
}

// =============================================================================
// BamlDecode implementation for BamlValue
// =============================================================================

use crate::{
    proto::baml_cffi_v1::{
        cffi_field_type_literal, cffi_value_holder, CffiStreamState, CffiValueHolder,
    },
    types::{Check, CheckStatus, StreamingState},
};

impl<T: KnownTypes, S: KnownTypes> BamlDecode for BamlValue<T, S> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::NullValue(_)) | None => Ok(BamlValue::Null),
            Some(cffi_value_holder::Value::StringValue(s)) => Ok(BamlValue::String(s.clone())),
            Some(cffi_value_holder::Value::IntValue(i)) => Ok(BamlValue::Int(*i)),
            Some(cffi_value_holder::Value::FloatValue(f)) => Ok(BamlValue::Float(*f)),
            Some(cffi_value_holder::Value::BoolValue(b)) => Ok(BamlValue::Bool(*b)),
            Some(cffi_value_holder::Value::ListValue(list)) => {
                let items = list
                    .items
                    .iter()
                    .map(Self::baml_decode)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(BamlValue::List(items))
            }
            Some(cffi_value_holder::Value::MapValue(map)) => {
                let mut result = HashMap::new();
                for entry in &map.entries {
                    let value = entry
                        .value
                        .as_ref()
                        .ok_or_else(|| BamlError::internal("map entry missing value"))?;
                    result.insert(entry.key.clone(), Self::baml_decode(value)?);
                }
                Ok(BamlValue::Map(result))
            }
            Some(cffi_value_holder::Value::ClassValue(class)) => {
                // Decode as DynamicClass - FromBamlValue for known types can extract
                let name = class
                    .name
                    .as_ref()
                    .map(|n| n.name.clone())
                    .unwrap_or_default();
                let mut fields = HashMap::new();
                for entry in &class.fields {
                    if let Some(value) = &entry.value {
                        fields.insert(entry.key.clone(), Self::baml_decode(value)?);
                    }
                }
                Ok(BamlValue::DynamicClass(DynamicClass::with_fields(
                    name, fields,
                )))
            }
            Some(cffi_value_holder::Value::EnumValue(e)) => {
                let name = e.name.as_ref().map(|n| n.name.clone()).unwrap_or_default();
                Ok(BamlValue::DynamicEnum(DynamicEnum {
                    name,
                    value: e.value.clone(),
                }))
            }
            Some(cffi_value_holder::Value::UnionVariantValue(union)) => {
                let inner = union
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("union variant missing value"))?;
                let decoded_value = Self::baml_decode(inner)?;

                // For simple optional types (like string?), unwrap directly
                // is_single_pattern = true means it's a simple A | null pattern
                if union.is_single_pattern {
                    Ok(decoded_value)
                } else {
                    // Complex unions need DynamicUnion wrapper for discriminated access
                    let name = union
                        .name
                        .as_ref()
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let variant_name = union.value_option_name.clone();
                    Ok(BamlValue::DynamicUnion(DynamicUnion {
                        name,
                        variant_name,
                        value: Box::new(decoded_value),
                    }))
                }
            }
            Some(cffi_value_holder::Value::CheckedValue(checked)) => {
                let inner = checked
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("checked value missing inner"))?;
                let value = Box::new(Self::baml_decode(inner)?);
                let checks = checked
                    .checks
                    .iter()
                    .map(|c| {
                        (
                            c.name.clone(),
                            Check {
                                name: c.name.clone(),
                                expression: c.expression.clone(),
                                status: match c.status.as_str() {
                                    "passed" | "PASSED" => CheckStatus::Succeeded,
                                    _ => CheckStatus::Failed,
                                },
                            },
                        )
                    })
                    .collect();
                Ok(BamlValue::Checked(Checked { value, checks }))
            }
            Some(cffi_value_holder::Value::StreamingStateValue(ss)) => {
                let inner = ss
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("stream state missing value"))?;
                let value = Box::new(Self::baml_decode(inner)?);
                let state = match ss.state() {
                    CffiStreamState::Pending => StreamingState::Pending,
                    CffiStreamState::Started => StreamingState::Started,
                    CffiStreamState::Done => StreamingState::Done,
                };
                Ok(BamlValue::StreamState(StreamState { value, state }))
            }
            Some(cffi_value_holder::Value::LiteralValue(lit)) => {
                // Literals decode to their underlying primitive
                match &lit.literal {
                    Some(cffi_field_type_literal::Literal::StringLiteral(s)) => {
                        Ok(BamlValue::String(s.value.clone()))
                    }
                    Some(cffi_field_type_literal::Literal::IntLiteral(i)) => {
                        Ok(BamlValue::Int(i.value))
                    }
                    Some(cffi_field_type_literal::Literal::BoolLiteral(b)) => {
                        Ok(BamlValue::Bool(b.value))
                    }
                    None => Ok(BamlValue::Null),
                }
            }
            Some(cffi_value_holder::Value::ObjectValue(_)) => {
                // ObjectValue is for FFI handles (Media, TypeBuilder), not decodable
                Err(BamlError::internal(
                    "ObjectValue cannot be decoded to BamlValue",
                ))
            }
        }
    }
}
