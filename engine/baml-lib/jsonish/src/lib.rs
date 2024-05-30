#[cfg(test)]
mod tests;

use std::collections::HashMap;

use anyhow::Result;
mod deserializer;
mod jsonish;

use baml_types::FieldType;
use deserializer::coercer::{ParsingContext, TypeCoercer};

use internal_baml_core::ir::repr::IntermediateRepr;

pub use deserializer::types::BamlValueWithFlags;

pub fn from_str(
    ir: &IntermediateRepr,
    env: &HashMap<String, String>,
    target: &FieldType,
    raw_string: &str,
    allow_partials: bool,
) -> Result<BamlValueWithFlags> {
    if matches!(target, FieldType::Primitive(String)) {
        return Ok(BamlValueWithFlags::String(raw_string.to_string().into()));
    }

    // When the schema is just a string, i should really just return the raw_string w/o parsing it.
    let value = jsonish::parse(raw_string, jsonish::ParseOptions::default())?;

    log::info!("Parsed value: {:?}", value);

    let ctx = ParsingContext::new(ir, env, allow_partials);

    // Lets try to now coerce the value into the expected schema.
    match target.coerce(&ctx, target, Some(&value)) {
        Ok(v) => Ok(v),
        Err(e) => anyhow::bail!("Failed to coerce value: {}", e),
    }
}
