//! Primitive type BamlDecode and BamlEncode implementations.

use crate::__internal::cffi_field_type_literal;
use crate::error::BamlError;
use crate::proto::baml_cffi_v1::{
    CffiValueHolder, HostListValue, HostMapEntry, HostMapValue, HostValue, cffi_value_holder,
    host_map_entry, host_value,
};
use serde_json::Value as JsonValue;

use super::helpers::variant_name;
use super::traits::{BamlDecode, BamlEncode};

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
                "expected null/void, got {:?}",
                other.as_ref().map(variant_name)
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

/// HostValue is already encoded, return as-is
impl BamlEncode for HostValue {
    fn baml_encode(&self) -> HostValue {
        self.clone()
    }
}

/// Encode arbitrary JSON values for ClientRegistry options
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
