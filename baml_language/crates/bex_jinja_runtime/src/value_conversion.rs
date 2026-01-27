use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use minijinja::value::Value as JinjaValue;

use crate::MAGIC_MEDIA_DELIMITER;

/// Convert a `BexExternalValue` to a minijinja Value.
///
/// `BexExternalValue` is already fully extracted from the VM heap,
/// so no heap access is needed here.
pub(crate) fn external_value_to_jinja(value: &BexExternalValue) -> JinjaValue {
    match value {
        BexExternalValue::Null => JinjaValue::from(()), // Maps to None in Jinja
        BexExternalValue::Int(i) => JinjaValue::from(*i),
        BexExternalValue::Float(f) => JinjaValue::from(*f),
        BexExternalValue::Bool(b) => JinjaValue::from(*b),
        BexExternalValue::String(s) => JinjaValue::from(s.as_str()),

        BexExternalValue::Array { items, .. } => {
            let jinja_items: Vec<JinjaValue> = items.iter().map(external_value_to_jinja).collect();
            JinjaValue::from(jinja_items)
        }

        BexExternalValue::Map { entries, .. } => {
            let jinja_map: IndexMap<String, JinjaValue> = entries
                .iter()
                .map(|(k, v)| (k.clone(), external_value_to_jinja(v)))
                .collect();
            JinjaValue::from_iter(jinja_map)
        }

        BexExternalValue::Instance { fields, .. } => {
            // Convert instance fields to a map for Jinja access
            let jinja_map: IndexMap<String, JinjaValue> = fields
                .iter()
                .map(|(k, v)| (k.clone(), external_value_to_jinja(v)))
                .collect();
            JinjaValue::from_iter(jinja_map)
        }

        BexExternalValue::Variant {
            variant_name,
            enum_name: _,
        } => {
            // Enum variants are rendered as their variant name
            JinjaValue::from(variant_name.as_str())
        }

        BexExternalValue::Union { value, .. } => {
            // Unwrap the union and convert the inner value
            external_value_to_jinja(value)
        }

        BexExternalValue::Media { .. } => {
            // TODO: Media handling will be implemented in a separate pass.
            // For now, stub out with a placeholder that will be parsed back.
            // The actual media resolution mechanism needs to be designed.
            let placeholder_handle: usize = 0; // Stubbed - real implementation TBD
            JinjaValue::from(format!(
                "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{placeholder_handle}:baml-end-media:{MAGIC_MEDIA_DELIMITER}"
            ))
        }

        BexExternalValue::Resource(_) => {
            // Resources shouldn't appear in template arguments
            JinjaValue::from("[Resource]")
        }
    }
}
