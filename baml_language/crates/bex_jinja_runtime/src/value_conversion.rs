use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use minijinja::value::Value as JinjaValue;

use crate::MAGIC_MEDIA_DELIMITER;

/// Media lookup table entry: maps a usize index to an external Handle + MediaKind.
pub(crate) type MediaTable = Vec<(bex_external_types::Handle, baml_base::MediaKind)>;

/// Convert a `BexExternalValue` to a minijinja Value.
///
/// `BexExternalValue` is already fully extracted from the VM heap,
/// so no heap access is needed here.
///
/// Media values are registered in `media_table` and embedded as magic delimiter strings
/// containing the table index. After Jinja rendering, the indices are resolved back
/// to `Handle` + `MediaKind` pairs.
pub(crate) fn external_value_to_jinja(
    value: &BexExternalValue,
    media_table: &mut MediaTable,
) -> JinjaValue {
    match value {
        BexExternalValue::Null => JinjaValue::from(()), // Maps to None in Jinja
        BexExternalValue::Int(i) => JinjaValue::from(*i),
        BexExternalValue::Float(f) => JinjaValue::from(*f),
        BexExternalValue::Bool(b) => JinjaValue::from(*b),
        BexExternalValue::String(s) => JinjaValue::from(s.as_str()),

        BexExternalValue::Array { items, .. } => {
            let jinja_items: Vec<JinjaValue> = items
                .iter()
                .map(|item| external_value_to_jinja(item, media_table))
                .collect();
            JinjaValue::from(jinja_items)
        }

        BexExternalValue::Map { entries, .. } => {
            let jinja_map: IndexMap<String, JinjaValue> = entries
                .iter()
                .map(|(k, v)| (k.clone(), external_value_to_jinja(v, media_table)))
                .collect();
            JinjaValue::from_iter(jinja_map)
        }

        BexExternalValue::Instance { fields, .. } => {
            // Convert instance fields to a map for Jinja access
            let jinja_map: IndexMap<String, JinjaValue> = fields
                .iter()
                .map(|(k, v)| (k.clone(), external_value_to_jinja(v, media_table)))
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
            external_value_to_jinja(value, media_table)
        }

        BexExternalValue::Media { handle, kind } => {
            // Register in lookup table and embed index as magic delimiter string
            let index = media_table.len();
            media_table.push((handle.clone(), *kind));
            JinjaValue::from(format!(
                "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{index}:baml-end-media:{MAGIC_MEDIA_DELIMITER}"
            ))
        }

        BexExternalValue::Resource(_) => {
            // Resources shouldn't appear in template arguments
            JinjaValue::from("[Resource]")
        }

        BexExternalValue::PromptAst(_) => {
            // PromptAst shouldn't appear in template arguments
            JinjaValue::from("[PromptAst]")
        }

        BexExternalValue::PrimitiveClient(_) => {
            // PrimitiveClient shouldn't appear in template arguments
            JinjaValue::from("[PrimitiveClient]")
        }

        BexExternalValue::FunctionRef { .. } => {
            // FunctionRef shouldn't appear in template arguments
            JinjaValue::from("[Function]")
        }
    }
}
