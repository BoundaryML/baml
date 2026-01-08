//! Primitive type `BamlDecode` and `BamlEncode` implementations.

use std::collections::HashMap;

use serde_json::Value as JsonValue;

use super::{
    helpers::variant_name,
    traits::{BamlDecode, BamlEncode},
};
use crate::{
    __internal::cffi_field_type_literal,
    error::BamlError,
    proto::baml_cffi_v1::{
        cffi_value_holder, host_map_entry, host_value, CffiValueHolder, HostListValue,
        HostMapEntry, HostMapValue, HostValue,
    },
};

// =============================================================================
// Primitive BamlDecode implementations
// =============================================================================

impl BamlDecode for String {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::StringValue(s)) => Ok(s.clone()),
            Some(cffi_value_holder::Value::LiteralValue(l)) => match &l.literal {
                Some(cffi_field_type_literal::Literal::StringLiteral(s)) => Ok(s.value.clone()),
                _ => Err(BamlError::internal(format!(
                    "expected string, got {:?}",
                    holder.value.as_ref().map(variant_name)
                ))),
            },
            other => Err(BamlError::internal(format!(
                "expected string, got {:?}",
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

impl BamlDecode for i64 {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::IntValue(i)) => Ok(*i),
            Some(cffi_value_holder::Value::LiteralValue(l)) => match &l.literal {
                Some(cffi_field_type_literal::Literal::IntLiteral(i)) => Ok(i.value),
                _ => Err(BamlError::internal(format!(
                    "expected int, got {:?}",
                    holder.value.as_ref().map(variant_name)
                ))),
            },
            other => Err(BamlError::internal(format!(
                "expected int, got {:?}",
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

impl BamlDecode for f64 {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::FloatValue(f)) => Ok(*f),
            other => Err(BamlError::internal(format!(
                "expected float, got {:?}",
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

impl BamlDecode for bool {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::BoolValue(b)) => Ok(*b),
            Some(cffi_value_holder::Value::LiteralValue(l)) => match &l.literal {
                Some(cffi_field_type_literal::Literal::BoolLiteral(b)) => Ok(b.value),
                _ => Err(BamlError::internal(format!(
                    "expected bool, got {:?}",
                    holder.value.as_ref().map(variant_name)
                ))),
            },
            other => Err(BamlError::internal(format!(
                "expected bool, got {:?}",
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

/// Unit type decodes from null or empty values (for void method returns)
impl BamlDecode for () {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::NullValue(_)) | None => Ok(()),
            other => Err(BamlError::internal(format!(
                "expected null/void, got {:?} - {:?}",
                other.as_ref().map(variant_name),
                holder,
            ))),
        }
    }
}

// =============================================================================
// Primitive BamlEncode implementations
// =============================================================================

impl BamlEncode for String {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::StringValue(self.clone())),
        }
    }
}

impl BamlEncode for &str {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::StringValue((*self).to_string())),
        }
    }
}

impl BamlEncode for i64 {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::IntValue(*self)),
        }
    }
}

impl BamlEncode for i32 {
    fn baml_encode(&self) -> HostValue {
        i64::from(*self).baml_encode()
    }
}

impl BamlEncode for f64 {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::FloatValue(*self)),
        }
    }
}

impl BamlEncode for bool {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::BoolValue(*self)),
        }
    }
}

/// Unit type encodes to null (for void method parameters)
impl BamlEncode for () {
    fn baml_encode(&self) -> HostValue {
        HostValue { value: None }
    }
}

/// Blanket impl for references to encodable types
impl<T: BamlEncode> BamlEncode for &T {
    fn baml_encode(&self) -> HostValue {
        (*self).baml_encode()
    }
}

/// `HostValue` is already encoded, return as-is
impl BamlEncode for HostValue {
    fn baml_encode(&self) -> HostValue {
        self.clone()
    }
}

/// Encode arbitrary JSON values for `ClientRegistry` options
impl BamlEncode for JsonValue {
    fn baml_encode(&self) -> HostValue {
        let inner = match self {
            JsonValue::Null => None,
            JsonValue::Bool(b) => Some(host_value::Value::BoolValue(*b)),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(host_value::Value::IntValue(i))
                } else if let Some(f) = n.as_f64() {
                    Some(host_value::Value::FloatValue(f))
                } else {
                    // Fallback to string representation for u64 values
                    Some(host_value::Value::StringValue(n.to_string()))
                }
            }
            JsonValue::String(s) => Some(host_value::Value::StringValue(s.clone())),
            JsonValue::Array(arr) => {
                let values = arr.iter().map(BamlEncode::baml_encode).collect();
                Some(host_value::Value::ListValue(HostListValue { values }))
            }
            JsonValue::Object(obj) => {
                let entries = obj
                    .iter()
                    .map(|(k, v)| HostMapEntry {
                        key: Some(host_map_entry::Key::StringKey(k.clone())),
                        value: Some(v.baml_encode()),
                    })
                    .collect();
                Some(host_value::Value::MapValue(HostMapValue { entries }))
            }
        };

        HostValue { value: inner }
    }
}

impl BamlDecode for JsonValue {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::NullValue(_)) | None => Ok(JsonValue::Null),
            Some(cffi_value_holder::Value::StringValue(s)) => Ok(JsonValue::String(s.clone())),
            Some(cffi_value_holder::Value::IntValue(i)) => Ok(JsonValue::Number((*i).into())),
            Some(cffi_value_holder::Value::FloatValue(f)) => {
                if let Some(number) = serde_json::Number::from_f64(*f) {
                    Ok(JsonValue::Number(number))
                } else {
                    Err(BamlError::internal(format!(
                        "failed to convert float to json number: {}",
                        *f
                    )))
                }
            }
            Some(cffi_value_holder::Value::BoolValue(b)) => Ok(JsonValue::Bool(*b)),
            Some(cffi_value_holder::Value::ListValue(list)) => {
                let items = list
                    .items
                    .iter()
                    .map(Self::baml_decode)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(JsonValue::Array(items))
            }
            Some(cffi_value_holder::Value::MapValue(map)) => {
                let mut result = serde_json::Map::new();
                for entry in &map.entries {
                    let value = entry
                        .value
                        .as_ref()
                        .ok_or_else(|| BamlError::internal("map entry missing value"))?;
                    result.insert(entry.key.clone(), Self::baml_decode(value)?);
                }
                Ok(JsonValue::Object(result))
            }
            Some(cffi_value_holder::Value::ClassValue(class)) => {
                // We explicitly drop class names here. Its json! no types!
                // let name = class
                //     .name
                //     .as_ref()
                //     .map(|n| n.name.clone())
                //     .unwrap_or_default();

                let mut fields = serde_json::Map::new();
                for entry in &class.fields {
                    if let Some(value) = &entry.value {
                        fields.insert(entry.key.clone(), Self::baml_decode(value)?);
                    }
                }
                Ok(JsonValue::Object(fields))
            }
            Some(cffi_value_holder::Value::EnumValue(e)) => {
                // We explicitly drop enum names here. Its json! no types!
                // let name = e.name.as_ref().map(|n| n.name.clone()).unwrap_or_default();
                Ok(JsonValue::String(e.value.clone()))
            }
            Some(cffi_value_holder::Value::UnionVariantValue(union)) => {
                let inner = union
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("union variant missing value"))?;
                let decoded_value = Self::baml_decode(inner)?;
                // We drop union-ness here! its json! no types!
                Ok(decoded_value)
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
                            crate::Check {
                                name: c.name.clone(),
                                expression: c.expression.clone(),
                                status: match c.status.as_str() {
                                    "passed" | "PASSED" => crate::CheckStatus::Succeeded,
                                    _ => crate::CheckStatus::Failed,
                                },
                            },
                        )
                    })
                    .collect::<HashMap<String, crate::Check>>();
                Ok(serde_json::json!({ "value": value, "checks": checks }))
            }
            Some(cffi_value_holder::Value::StreamingStateValue(ss)) => {
                let inner = ss
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("stream state missing value"))?;
                let value = Box::new(Self::baml_decode(inner)?);
                let state = match ss.state() {
                    crate::__internal::CffiStreamState::Pending => crate::StreamingState::Pending,
                    crate::__internal::CffiStreamState::Started => crate::StreamingState::Started,
                    crate::__internal::CffiStreamState::Done => crate::StreamingState::Done,
                };
                Ok(serde_json::json!({ "value": value, "state": state }))
            }
            Some(cffi_value_holder::Value::LiteralValue(lit)) => {
                // Literals decode to their underlying primitive
                match &lit.literal {
                    Some(cffi_field_type_literal::Literal::StringLiteral(s)) => {
                        Ok(JsonValue::String(s.value.clone()))
                    }
                    Some(cffi_field_type_literal::Literal::IntLiteral(i)) => {
                        Ok(JsonValue::Number(i.value.into()))
                    }
                    Some(cffi_field_type_literal::Literal::BoolLiteral(b)) => {
                        Ok(JsonValue::Bool(b.value))
                    }
                    None => Ok(JsonValue::Null),
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
