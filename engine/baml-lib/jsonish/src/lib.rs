#[cfg(test)]
mod tests;

use anyhow::Result;
pub mod deserializer;
mod jsonish;

use baml_types::FieldType;
use deserializer::coercer::{ParsingContext, TypeCoercer};

pub use deserializer::types::BamlValueWithFlags;
use internal_baml_core::ir::TypeValue;
use internal_baml_jinja::types::OutputFormatContent;

use deserializer::deserialize_flags::Flag;

pub fn from_str(
    of: &OutputFormatContent,
    target: &FieldType,
    raw_string: &str,
    allow_partials: bool,
) -> Result<BamlValueWithFlags> {
    if matches!(target, FieldType::Primitive(TypeValue::String)) {
        return Ok(BamlValueWithFlags::String(raw_string.to_string().into()));
    }

    // When the schema is just a string, i should really just return the raw_string w/o parsing it.
    let value = jsonish::parse(raw_string, jsonish::ParseOptions::default())?;
    // let schema = deserializer::schema::from_jsonish_value(&value, None);

    // Pick the schema that is the most specific.
    // log::info!("Parsed: {}", schema);
    log::debug!("Parsed JSONish (step 1 of parsing): {:#?}", value);
    let ctx = ParsingContext::new(of, allow_partials);
    // let res = schema.cast_to(target);
    // log::info!("Casted: {:?}", res);

    // match res {
    //     Ok(v) => Ok(v),
    //     Err(e) => anyhow::bail!("Failed to cast value: {}", e),
    // }

    // Determine the best way to get the desired schema from the parsed schema.

    // Lets try to now coerce the value into the expected schema.
    match target.coerce(&ctx, target, Some(&value)) {
        Ok(v) => {
            if v.conditions()
                .flags()
                .iter()
                .any(|f| matches!(f, Flag::InferedObject(jsonish::Value::String(_))))
            {
                anyhow::bail!("Failed to coerce value: {:?}", v.conditions().flags());
            }

            Ok(v)
        }
        Err(e) => anyhow::bail!("Failed to coerce value: {}", e),
    }
}
