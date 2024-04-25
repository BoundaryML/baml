#[cfg(test)]
mod tests;

use anyhow::Result;
mod deserializer;
mod json_schema;

use serde_json::{self};

pub fn from_str<I: AsRef<str>>(raw_string: &str, schema: I) -> Result<serde_json::Value> {
    let schema = serde_json::from_str::<json_schema::JSONSchema7>(schema.as_ref())?;

    // let s = jsonschema::JSONSchema::compile(&schema)?;
    // let target = deserializer::Target {
    //     schema: serde_json::from_str::<deserializer::Type>(schema.as_ref())?,
    // };
    // target.parse(raw_string)

    // When the schema is just a string, i should really just return the raw_string w/o parsing it.
    let value =
        deserializer::parse_jsonish_value(raw_string, deserializer::JSONishOptions::default())?;

    schema.coerce(&value)
    // Lets try to now coerce the value into the expected schema.
}
