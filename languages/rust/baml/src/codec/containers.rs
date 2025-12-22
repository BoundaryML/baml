//! Container type BamlDecode and BamlEncode implementations.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::error::BamlError;
use crate::proto::baml_cffi_v1::{
    cffi_value_holder, host_map_entry, host_value, CffiValueHolder, HostListValue, HostMapEntry,
    HostMapValue, HostValue,
};

use super::helpers::variant_name;
use super::traits::{BamlDecode, BamlEncode};

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

/// Impl for slices of references
impl<T: BamlEncode> BamlEncode for &[&T] {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::ListValue(HostListValue {
                values: self.iter().map(|v| v.baml_encode()).collect(),
            })),
        }
    }
}
