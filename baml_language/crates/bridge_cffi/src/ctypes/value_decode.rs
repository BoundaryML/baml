//! Decode C FFI / protobuf host values into `BexExternalValue`.
//!
//! Converts `HostValue` (from the C bridge) to the engine's `BexExternalValue` representation
//! so the BEX engine can use them as function arguments.

use bex_factory::{BexExternalValue, Ty};
use indexmap::IndexMap;

use crate::{
    baml::cffi::{
        HostClassValue, HostEnumValue, HostListValue, HostMapEntry, HostMapValue, HostValue,
        host_value::Value as HostValueVariant,
    },
    error::BridgeError,
};

/// Decode a protobuf `HostValue` into a `BexExternalValue` for use by the BEX engine.
pub fn host_value_to_external(value: HostValue) -> Result<BexExternalValue, BridgeError> {
    match value.value {
        None => Ok(BexExternalValue::Null),
        Some(variant) => match variant {
            HostValueVariant::StringValue(s) => Ok(BexExternalValue::String(s)),
            HostValueVariant::IntValue(i) => Ok(BexExternalValue::Int(i)),
            HostValueVariant::FloatValue(f) => Ok(BexExternalValue::Float(f)),
            HostValueVariant::BoolValue(b) => Ok(BexExternalValue::Bool(b)),
            HostValueVariant::ListValue(list) => convert_list(list),
            HostValueVariant::MapValue(map) => convert_map(map),
            HostValueVariant::ClassValue(class) => convert_class(class),
            HostValueVariant::EnumValue(e) => convert_enum(e),
            HostValueVariant::Handle(_handle) => Err(BridgeError::HandleNotSupported),
        },
    }
}

fn convert_list(list: HostListValue) -> Result<BexExternalValue, BridgeError> {
    let items: Result<Vec<BexExternalValue>, BridgeError> = list
        .values
        .into_iter()
        .map(host_value_to_external)
        .collect();
    Ok(BexExternalValue::Array {
        // Type info not in protobuf, use Union of all possible types as fallback
        element_type: Ty::Union(vec![Ty::Int, Ty::Float, Ty::String, Ty::Bool, Ty::Null]),
        items: items?,
    })
}

fn convert_map(map: HostMapValue) -> Result<BexExternalValue, BridgeError> {
    let mut entries = IndexMap::new();
    for entry in map.entries {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(host_value_to_external)
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        entries.insert(key, value);
    }
    Ok(BexExternalValue::Map {
        key_type: Ty::String,
        // Type info not in protobuf, use Union of all possible types as fallback
        value_type: Ty::Union(vec![Ty::Int, Ty::Float, Ty::String, Ty::Bool, Ty::Null]),
        entries,
    })
}

fn convert_class(class: HostClassValue) -> Result<BexExternalValue, BridgeError> {
    let mut fields = IndexMap::new();
    for entry in class.fields {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(host_value_to_external)
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        fields.insert(key, value);
    }
    Ok(BexExternalValue::Instance {
        class_name: class.name,
        fields,
    })
}

fn convert_enum(e: HostEnumValue) -> Result<BexExternalValue, BridgeError> {
    Ok(BexExternalValue::Variant {
        enum_name: e.name,
        variant_name: e.value,
    })
}

fn extract_string_key(entry: &HostMapEntry) -> Result<String, BridgeError> {
    use crate::baml::cffi::host_map_entry::Key;
    match &entry.key {
        Some(Key::StringKey(s)) => Ok(s.clone()),
        Some(Key::IntKey(i)) => Ok(i.to_string()),
        Some(Key::BoolKey(b)) => Ok(b.to_string()),
        Some(Key::EnumKey(e)) => Ok(format!("{}::{}", e.name, e.value)),
        None => Err(BridgeError::MapEntryMissingKey),
    }
}

/// Decode protobuf kwargs into an `IndexMap<String, BexExternalValue>` for engine call arguments.
pub fn kwargs_to_bex_values(
    kwargs: Vec<HostMapEntry>,
) -> Result<IndexMap<String, BexExternalValue>, BridgeError> {
    let mut result = IndexMap::new();
    for entry in kwargs {
        let key = extract_string_key(&entry)?;
        let value = entry
            .value
            .map(host_value_to_external)
            .transpose()?
            .unwrap_or(BexExternalValue::Null);
        result.insert(key, value);
    }
    Ok(result)
}
