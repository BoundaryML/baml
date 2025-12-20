use std::borrow::Cow;
use std::collections::HashMap;

use crate::error::BamlError;
use crate::proto::baml_cffi_v1::{
    cffi_value_holder, host_map_entry, host_value, CffiMapEntry, CffiValueClass, CffiValueHolder,
    HostClassValue, HostEnumValue, HostListValue, HostMapEntry, HostMapValue, HostValue,
};

/// Trait for decoding from CFFI protobuf format (BAML -> Rust)
pub trait BamlDecode: Sized {
    /// Decode from a CffiValueHolder (outbound schema)
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError>;
}

/// Trait for encoding to CFFI protobuf format (Rust -> BAML)
pub trait BamlEncode {
    /// Encode to a HostValue (inbound schema)
    fn baml_encode(&self) -> HostValue;
}

// =============================================================================
// Union variant unwrapping helper
// =============================================================================

/// Unwrap single-pattern union variants (e.g., optional fields during streaming).
///
/// BAML wraps values in UnionVariantValue with is_single_pattern=true for optional types.
/// This function recursively unwraps these wrappers to get to the actual value.
fn unwrap_single_pattern_union(holder: &CffiValueHolder) -> Cow<'_, CffiValueHolder> {
    match &holder.value {
        Some(cffi_value_holder::Value::UnionVariantValue(union)) if union.is_single_pattern => {
            match &union.value {
                Some(inner) => {
                    // Recursively unwrap in case of nested wrappers
                    match unwrap_single_pattern_union(inner) {
                        Cow::Borrowed(h) => Cow::Borrowed(h),
                        Cow::Owned(h) => Cow::Owned(h),
                    }
                }
                None => Cow::Borrowed(holder),
            }
        }
        _ => Cow::Borrowed(holder),
    }
}

// =============================================================================
// Primitive BamlDecode implementations
// =============================================================================

impl BamlDecode for String {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::StringValue(s)) => Ok(s.clone()),
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
// Container BamlDecode implementations
// =============================================================================

impl<T: BamlDecode> BamlDecode for Vec<T> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::ListValue(list)) => {
                list.items.iter().map(T::baml_decode).collect()
            }
            other => Err(BamlError::internal(format!(
                "expected list, got {:?}",
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

impl<T: BamlDecode> BamlDecode for Option<T> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        // First unwrap any single-pattern union wrappers
        let holder = unwrap_single_pattern_union(holder);
        match &holder.value {
            Some(cffi_value_holder::Value::NullValue(_)) | None => Ok(None),
            _ => Ok(Some(T::baml_decode(&holder)?)),
        }
    }
}

impl<V: BamlDecode> BamlDecode for HashMap<String, V> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::MapValue(map)) => {
                let mut result = HashMap::new();
                for entry in &map.entries {
                    let value = entry
                        .value
                        .as_ref()
                        .ok_or_else(|| BamlError::internal("map entry missing value"))?;
                    result.insert(entry.key.clone(), V::baml_decode(value)?);
                }
                Ok(result)
            }
            other => Err(BamlError::internal(format!(
                "expected map, got {:?}",
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

// =============================================================================
// Container BamlEncode implementations
// =============================================================================

impl<T: BamlEncode> BamlEncode for Vec<T> {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::ListValue(HostListValue {
                values: self.iter().map(BamlEncode::baml_encode).collect(),
            })),
        }
    }
}

impl<T: BamlEncode> BamlEncode for Option<T> {
    fn baml_encode(&self) -> HostValue {
        match self {
            Some(v) => v.baml_encode(),
            None => HostValue { value: None },
        }
    }
}

impl<V: BamlEncode> BamlEncode for HashMap<String, V> {
    fn baml_encode(&self) -> HostValue {
        let entries: Vec<HostMapEntry> = self
            .iter()
            .map(|(k, v)| HostMapEntry {
                key: Some(host_map_entry::Key::StringKey(k.clone())),
                value: Some(v.baml_encode()),
            })
            .collect();
        HostValue {
            value: Some(host_value::Value::MapValue(HostMapValue { entries })),
        }
    }
}

// =============================================================================
// Helper traits for generated code
// =============================================================================

/// Helper trait for decoding BAML classes
pub trait BamlClass: Sized {
    /// Expected BAML type name
    const TYPE_NAME: &'static str;

    /// Decode from class value
    fn from_class_value(class: &CffiValueClass) -> Result<Self, BamlError>;
}

impl<T: BamlClass> BamlDecode for T {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::ClassValue(class)) => T::from_class_value(class),
            other => Err(BamlError::internal(format!(
                "expected class {}, got {:?}",
                T::TYPE_NAME,
                other.as_ref().map(variant_name)
            ))),
        }
    }
}

/// Helper trait for decoding BAML enums
pub trait BamlEnum: Sized {
    /// Expected BAML enum name
    const ENUM_NAME: &'static str;

    /// Decode from string variant name
    fn from_variant_name(name: &str) -> Result<Self, BamlError>;
}

// Note: BamlEnum doesn't auto-impl BamlDecode because enums need special handling
// in generated code to support both regular and dynamic enums

/// Decode an enum from a CffiValueHolder
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

/// Helper to get variant name for error messages
fn variant_name(v: &cffi_value_holder::Value) -> &'static str {
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

/// Encode a class to HostValue
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

/// Encode an enum to HostValue
pub fn encode_enum(enum_name: &str, variant: &str) -> HostValue {
    HostValue {
        value: Some(host_value::Value::EnumValue(HostEnumValue {
            name: enum_name.to_string(),
            value: variant.to_string(),
        })),
    }
}

// =============================================================================
// Kwargs encoding for method calls
// =============================================================================

/// Trait for types that can be converted to method kwargs.
///
/// This allows ergonomic method calls without manually constructing `HostMapEntry` vectors.
pub trait IntoKwargs {
    fn into_kwargs(self) -> Vec<HostMapEntry>;
}

/// Empty kwargs - for methods with no arguments
impl IntoKwargs for () {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![]
    }
}

/// Pre-built kwargs vector passes through
impl IntoKwargs for Vec<HostMapEntry> {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        self
    }
}

/// Single kwarg from tuple
impl<V: BamlEncode> IntoKwargs for (&str, V) {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![HostMapEntry {
            key: Some(host_map_entry::Key::StringKey(self.0.to_string())),
            value: Some(self.1.baml_encode()),
        }]
    }
}

/// Multiple kwargs from slice of tuples (up to reasonable sizes)
impl<V: BamlEncode + Clone> IntoKwargs for &[(&str, V)] {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        self.iter()
            .map(|(k, v)| HostMapEntry {
                key: Some(host_map_entry::Key::StringKey((*k).to_string())),
                value: Some(v.baml_encode()),
            })
            .collect()
    }
}

/// Two kwargs from tuple pair
impl<V1: BamlEncode, V2: BamlEncode> IntoKwargs for ((&str, V1), (&str, V2)) {
    fn into_kwargs(self) -> Vec<HostMapEntry> {
        vec![
            HostMapEntry {
                key: Some(host_map_entry::Key::StringKey(self.0.0.to_string())),
                value: Some(self.0.1.baml_encode()),
            },
            HostMapEntry {
                key: Some(host_map_entry::Key::StringKey(self.1.0.to_string())),
                value: Some(self.1.1.baml_encode()),
            },
        ]
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
                Some(holder) => T::baml_decode(holder),
                None => Err(BamlError::internal(format!(
                    "field '{}' has no value",
                    field_name
                ))),
            };
        }
    }
    Err(BamlError::internal(format!(
        "missing field '{}'",
        field_name
    )))
}

/// Helper for decoding an optional field from a class's fields map
pub fn decode_optional_field<T: BamlDecode>(
    fields: &[CffiMapEntry],
    field_name: &str,
) -> Result<Option<T>, BamlError> {
    for entry in fields {
        if entry.key == field_name {
            return match &entry.value {
                Some(holder) => match &holder.value {
                    Some(cffi_value_holder::Value::NullValue(_)) | None => Ok(None),
                    _ => Ok(Some(T::baml_decode(holder)?)),
                },
                None => Ok(None),
            };
        }
    }
    Ok(None)
}
