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
use jsonish::Value;

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
    let mut value = jsonish::parse(raw_string, jsonish::ParseOptions::default())?;
    // let schema = deserializer::schema::from_jsonish_value(&value, None);

    // See Note [Streaming Number Invalidation]
    if allow_partials {
        invalidate_numbers_in_progress(&mut value, raw_string);
    }

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

/// Nullify numbers that may still be streaming in.
///
/// See note [Streaming Number Invalidation]
fn invalidate_numbers_in_progress(value: &mut Value, raw_string: &str) {
    let ends_in_digit = raw_string
        .chars()
        .last()
        .map_or(false, |c| c.is_numeric() || c == '.');
    let last_values = last_value_as_number(value);
    if ends_in_digit {
        last_values.into_iter().for_each(|v| {
            *v = Value::Null;
        })
    }
}

// Find the "last" element of a Value and return a mutable pointer to it.
// There may be multiple last elements only in the case of `Value::Anyof`.
// Every other case returns 0 or 1 pointers.
//
// The algorithm for finding the last value has several cases:
//   - Base case: Raw values like `String` and `Number` are themselves the
//     last value.
//   - Simple compound case: The last value of a flat array is trivially
//     the array's last element. The last value of an Object is the last
//     key-value pair to be parsed. Because we store objects' key-value
//     pairs in the order in which they are defined in the input tokens,
//     we simply look up the last field from the list of fields.
//   - Inductive case for lists and objects: When a list or object contains
//     other lists or objects, the transitively last element of the parent
//     type is the last element of the last direct element.
//   - AnyOf case: AnyOf represents multiple `jsonish::Value` parses of the
//     token stream. We have to compute the last item of each variant, and
//     handle them all, because any one of them could be selected downstream
//     by the coercer. AnyOf is the reason this function returns a `Vec` of
//     references rather than an `Optional` reference.
fn last_value_as_number(value: &mut Value) -> Vec<&mut Value> {
    match value {
        Value::String(_) => vec![],
        Value::Number(_) => vec![value],
        Value::Boolean(_) => vec![],
        Value::Null => vec![],
        Value::Array(items) => items
            .last_mut()
            .map(|i| last_value_as_number(i))
            .unwrap_or_default(),
        Value::Object(fields) => fields
            .last_mut()
            .map(|(_k, v)| last_value_as_number(v))
            .unwrap_or_default(),
        Value::Markdown(_, sub_value) => last_value_as_number(sub_value),
        Value::FixedJson(fixed_val, _fixes) => last_value_as_number(fixed_val),
        Value::AnyOf(variants, _) => variants
            .into_iter()
            .map(|variant| last_value_as_number(variant))
            .flatten()
            .collect(),
    }
}

/*
 * Note: Streaming Number Invalidation
 *
 * Displaying partial results can be more harmful for certain datatypes,
 * like ints and floats, despite being useful for strings. This is because
 * the prefix of a number conveys something very differenet about the full
 * number than what the prefix of a string conveys about the full string.
 *
 * To prevent confusing users with streamed number prefixes, we have
 * implemented a specific and slightly hacky workaround, which we may replace by
 * something more robust in the future. (We won't spend time here describing
 * this future solution).
 *
 * Our temporary solution works like this:
 *   - Flexibly parse LLM responses into `jsonish::Value` as usual.
 *   - Determine whether the last tokens represent a number that might
 *     be extended by subsequent tokens.
 *   - If the last tokens represent an in-progress number, identify the part
 *     of the `jsonish::Value` that is currently being extended, and convert
 *     it to `jsonish::Value::Null`.
 *
 * This algorithm is implemented in `invalidate_numbers_in_progress`. Finding
 * the currently-in-progress part of the `jsonish::Value` structure is
 * implemented in `last_value_as_number`.
 *
 *
 * Consider these examples of streamed tokens and their parses into
 * `Value`:
 *
 *  - "123" => 123. This `Value::Number` will be rewritten to
 *    `Value::Null` because it is the final element in the ADT
 *    and the input string ends in a digit.
 *
 *  - "[123, 456" => [123, 456]. The `456` will be nulled.
 *  - "[123, 456]" => [123, 456]. No change.
 */
