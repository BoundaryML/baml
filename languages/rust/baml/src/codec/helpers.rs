//! Helper functions for encoding/decoding BAML values.

use super::traits::{BamlDecode, BamlEnum};
use crate::{
    error::BamlError,
    proto::baml_cffi_v1::{
        cffi_value_holder, host_map_entry, host_value, CffiMapEntry, CffiValueHolder,
        HostClassValue, HostEnumValue, HostMapEntry, HostValue,
    },
};

/// Helper to get variant name for error messages
pub(crate) fn variant_name(v: &cffi_value_holder::Value) -> &'static str {
    match v {
        cffi_value_holder::Value::NullValue(_) => "null",
        cffi_value_holder::Value::StringValue(_) => "string",
        cffi_value_holder::Value::IntValue(_) => "int",
        cffi_value_holder::Value::FloatValue(_) => "float",
        cffi_value_holder::Value::BoolValue(_) => "bool",
        cffi_value_holder::Value::ClassValue(_) => "class",
        cffi_value_holder::Value::EnumValue(_) => "enum",
        cffi_value_holder::Value::LiteralValue(_) => "literal",
        cffi_value_holder::Value::ObjectValue(_) => "object",
        cffi_value_holder::Value::ListValue(_) => "list",
        cffi_value_holder::Value::MapValue(_) => "map",
        cffi_value_holder::Value::UnionVariantValue(_) => "union",
        cffi_value_holder::Value::CheckedValue(_) => "checked",
        cffi_value_holder::Value::StreamingStateValue(_) => "streaming_state",
    }
}

// Note: BamlClass types get BamlDecode implemented by the derive macro
// directly. This avoids blanket impl conflicts with container types like
// Box<T>.

// Note: BamlEnum doesn't auto-impl BamlDecode because enums need special
// handling in generated code to support both regular and dynamic enums

/// Decode an enum from a `CffiValueHolder`
pub fn decode_enum<T: BamlEnum>(holder: &CffiValueHolder) -> Result<T, BamlError> {
    match &holder.value {
        Some(cffi_value_holder::Value::EnumValue(e)) => T::from_variant_name(&e.value),
        other => Err(BamlError::internal(format!(
            "expected enum {}, got {:?}",
            T::ENUM_NAME,
            other.as_ref().map(variant_name)
        ))),
    }
}

/// Encode a class to `HostValue`
pub fn encode_class(name: &str, fields: Vec<(&str, HostValue)>) -> HostValue {
    let entries = fields
        .into_iter()
        .map(|(k, v)| HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(k.to_string())),
            value: Some(v),
        })
        .collect();

    HostValue {
        value: Some(host_value::Value::ClassValue(HostClassValue {
            name: name.to_string(),
            fields: entries,
        })),
    }
}

/// Encode a class with dynamic field names (for dynamic classes that flatten
/// __dynamic fields)
pub fn encode_class_dynamic(name: &str, fields: Vec<(&str, HostValue)>) -> HostValue {
    let entries = fields
        .into_iter()
        .map(|(k, v)| HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(k.to_string())),
            value: Some(v),
        })
        .collect();

    HostValue {
        value: Some(host_value::Value::ClassValue(HostClassValue {
            name: name.to_string(),
            fields: entries,
        })),
    }
}

/// Encode an enum to `HostValue`
pub fn encode_enum(enum_name: &str, variant: &str) -> HostValue {
    HostValue {
        value: Some(host_value::Value::EnumValue(HostEnumValue {
            name: enum_name.to_string(),
            value: variant.to_string(),
        })),
    }
}

/// Helper for decoding a field from a class's fields map
pub fn decode_field<T: BamlDecode>(
    fields: &[CffiMapEntry],
    field_name: &str,
) -> Result<T, BamlError> {
    for entry in fields {
        if entry.key == field_name {
            return match &entry.value {
                Some(holder) => match T::baml_decode(holder) {
                    Ok(value) => Ok(value),
                    Err(e) => Err(BamlError::internal(format!(
                        "error decoding field '{field_name}': {e}"
                    ))),
                },
                None => Err(BamlError::internal(format!(
                    "field '{field_name}' has no value"
                ))),
            };
        }
    }
    Err(BamlError::internal(format!("missing field '{field_name}'")))
}
