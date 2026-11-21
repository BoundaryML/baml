//! `baml.yaml`: one YAML document read as a `json` value.
//!
//! The document is read by the crate the interpreter reads it with, and
//! what it holds that a `json` cannot (a key that is no string, a tag, a
//! number that is not finite) is refused. What is left is written as JSON
//! text, which `baml.json`'s own reader builds the value from: one builder
//! of `json` values, and the keys of a mapping stay in the order written.

use std::fmt::Write as _;

use serde::Deserialize as _;
use serde_yaml::Value;

use super::{NativeBamlYaml, Natives, Root, Scope, Str, Thrown, json};
use crate::layout::NAMED_YAML_PARSE_ERROR;

/// Append `value` to `out` as JSON text, or say why it is no `json`.
fn write_json(value: &Value, out: &mut String) -> Result<(), &'static str> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Value::Number(number) => {
            if let Some(int) = number.as_i64() {
                let _ = write!(out, "{int}");
            } else if number.is_u64() {
                return Err("YAML integer is outside BAML int range");
            } else {
                match number.as_f64() {
                    Some(float) if float.is_finite() => {
                        out.push_str(zmij::Buffer::new().format_finite(float));
                    }
                    Some(_) => {
                        return Err("YAML non-finite floats are not supported by baml.yaml.parse");
                    }
                    None => {
                        return Err("YAML number cannot be represented as a BAML int or float");
                    }
                }
            }
        }
        Value::String(text) => write_string(text, out),
        Value::Sequence(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                write_json(item, out)?;
            }
            out.push(']');
        }
        Value::Mapping(entries) => {
            out.push('{');
            for (index, (key, value)) in entries.iter().enumerate() {
                let Value::String(key) = key else {
                    return Err("YAML mappings must use string keys to fit baml.json.json");
                };
                if index != 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_json(value, out)?;
            }
            out.push('}');
        }
        Value::Tagged(_) => return Err("YAML tags are not supported by baml.yaml.parse"),
    }
    Ok(())
}

/// Append `text` to `out` as a JSON string.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            control if u32::from(control) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// The one document of `text` as JSON text: nothing for a stream without a
/// document, and the message of the error for what is refused.
fn json_text(text: &str) -> Result<Option<String>, String> {
    let mut documents = serde_yaml::Deserializer::from_str(text);
    let Some(first) = documents.next() else {
        return Ok(None);
    };
    let parsed = Value::deserialize(first).map_err(|error| error.to_string())?;
    if documents.next().is_some() {
        return Err(
            "YAML stream contains multiple documents; baml.yaml.parse accepts exactly one document"
                .to_owned(),
        );
    }
    let mut out = String::new();
    write_json(&parsed, &mut out)?;
    Ok(Some(out))
}

impl NativeBamlYaml for Natives {
    // Reading a document and building its value recurse as deep as it
    // nests, which is more than a task's stack keeps for the runtime.
    fn parse<'s>(scope: &mut Scope<'s>, s: Root<'s, Str>) -> Result<usize, Thrown<'s>> {
        scope.with_room(|scope| match json_text(s.get(scope).as_str()) {
            Ok(None) => Ok(0),
            Ok(Some(text)) => {
                let text = scope.alloc_str(&text);
                json::parse_rooted(scope, text)
            }
            Err(message) => {
                let message = scope.alloc_str(&message);
                let class = scope.named_class(NAMED_YAML_PARSE_ERROR);
                let mut error = scope.build(class);
                error.set(scope, 0, message);
                let error = error.finish();
                Err(scope.throw(error))
            }
        })
    }
}
