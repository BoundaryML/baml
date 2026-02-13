use bex_external_types::{BexExternalAdt, BexExternalValue};
use bex_vm_types::MediaValue;
use indexmap::IndexMap;
use minijinja::value::Value as JinjaValue;

use super::{MAGIC_MEDIA_DELIMITER, RenderPromptError};

/// Convert a `BexExternalValue` to a minijinja Value.
///
/// `BexExternalValue` is already fully extracted from the VM heap,
/// so no heap access is needed here.
pub(crate) fn external_value_to_jinja(
    value: &BexExternalValue,
    media_handles: &mut std::collections::HashMap<usize, MediaValue>,
) -> Result<JinjaValue, RenderPromptError> {
    match value {
        BexExternalValue::Handle(_) => Err(RenderPromptError::ConversionError {
            reason: "Handle should not be passed to Jinja templates".to_string(),
        }),
        BexExternalValue::Null => Ok(JinjaValue::from(())), // Maps to None in Jinja
        BexExternalValue::Int(i) => Ok(JinjaValue::from(*i)),
        BexExternalValue::Float(f) => Ok(JinjaValue::from(*f)),
        BexExternalValue::Bool(b) => Ok(JinjaValue::from(*b)),
        BexExternalValue::String(s) => Ok(JinjaValue::from(s.as_str())),

        BexExternalValue::Array { items, .. } => {
            let jinja_items: Vec<JinjaValue> = items
                .iter()
                .map(|item| external_value_to_jinja(item, media_handles))
                .collect::<Result<_, _>>()?;
            Ok(JinjaValue::from(jinja_items))
        }

        BexExternalValue::Map { entries, .. } => {
            let jinja_map: IndexMap<String, JinjaValue> = entries
                .iter()
                .map(|(k, v)| Ok((k.clone(), external_value_to_jinja(v, media_handles)?)))
                .collect::<Result<_, RenderPromptError>>()?;
            Ok(JinjaValue::from_iter(jinja_map))
        }

        BexExternalValue::Instance { fields, .. } => {
            // Convert instance fields to a map for Jinja access
            let jinja_map: IndexMap<String, JinjaValue> = fields
                .iter()
                .map(|(k, v)| Ok((k.clone(), external_value_to_jinja(v, media_handles)?)))
                .collect::<Result<_, RenderPromptError>>()?;
            Ok(JinjaValue::from_iter(jinja_map))
        }

        BexExternalValue::Variant {
            variant_name,
            enum_name: _,
        } => {
            // Enum variants are rendered as their variant name
            Ok(JinjaValue::from(variant_name.as_str()))
        }

        BexExternalValue::Union { value, .. } => {
            // Unwrap the union and convert the inner value
            external_value_to_jinja(value, media_handles)
        }

        BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
            let media_id = media.random_id;
            media_handles.insert(media_id, media.clone());
            Ok(JinjaValue::from(format!(
                "{MAGIC_MEDIA_DELIMITER}:baml-start-media:{media_id}:baml-end-media:{MAGIC_MEDIA_DELIMITER}"
            )))
        }

        BexExternalValue::Resource(_) => Err(RenderPromptError::ConversionError {
            reason: "Resource should not be passed to Jinja templates".to_string(),
        }),

        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
            Err(RenderPromptError::ConversionError {
                reason: "PromptAst should not be passed to Jinja templates".to_string(),
            })
        }

        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
            Err(RenderPromptError::ConversionError {
                reason: "Collector should not be passed to Jinja templates".to_string(),
            })
        }

        BexExternalValue::FunctionRef { .. } => Err(RenderPromptError::ConversionError {
            reason: "FunctionRef should not be passed to Jinja templates".to_string(),
        }),
    }
}
