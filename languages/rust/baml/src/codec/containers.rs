//! Container type BamlDecode and BamlEncode implementations.

use std::{borrow::Cow, collections::HashMap};

use super::{
    helpers::variant_name,
    traits::{BamlDecode, BamlEncode},
};
use crate::{
    error::BamlError,
    proto::baml_cffi_v1::{
        CffiValueHolder, HostListValue, HostMapEntry, HostMapValue, HostValue, cffi_value_holder,
        host_map_entry, host_value,
    },
};

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
        match &holder.value {
            // Handle explicit null value (holder.value is None)
            None => Ok(None),

            Some(cffi_value_holder::Value::NullValue(_)) => Ok(None),

            Some(cffi_value_holder::Value::UnionVariantValue(union)) => {
                // Check variant name - "null" means None
                if union.value_option_name == "null" {
                    return Ok(None);
                }

                // For Option<Union> types: is_optional=true AND is_single_pattern=false
                // The runtime encodes these as a single flattened UnionVariantValue where:
                //   - value_option_name is the union variant (e.g., "bool", "int", "string")
                //   - value is the primitive/inner value
                // We need to pass the ENTIRE holder to the inner union decoder so it can
                // extract the variant name and decode properly.
                if union.is_optional && !union.is_single_pattern {
                    return Ok(Some(T::baml_decode(holder)?));
                }

                // For other cases (is_single_pattern=true for optional non-union types),
                // extract the inner value and decode it.
                let inner = union
                    .value
                    .as_ref()
                    .map(|b| b.as_ref())
                    .ok_or_else(|| BamlError::internal(format!(
                        "Option: union variant missing inner value (name={:?}, variant={}, is_single_pattern={})",
                        union.name.as_ref().map(|n| &n.name),
                        union.value_option_name,
                        union.is_single_pattern,
                    )))?;

                // Decode the inner value as T
                Ok(Some(T::baml_decode(inner)?))
            }
            _ => {
                // Try to decode as T (for non-union optional types)
                Ok(Some(T::baml_decode(holder)?))
            }
        }
    }
}

impl<T: BamlDecode> BamlDecode for Box<T> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        Ok(Box::new(T::baml_decode(holder)?))
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

impl<T: BamlEncode> BamlEncode for Box<T> {
    fn baml_encode(&self) -> HostValue {
        self.as_ref().baml_encode()
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

/// Impl for slices
impl<T: BamlEncode> BamlEncode for &[T] {
    fn baml_encode(&self) -> HostValue {
        HostValue {
            value: Some(host_value::Value::ListValue(HostListValue {
                values: self.iter().map(BamlEncode::baml_encode).collect(),
            })),
        }
    }
}
