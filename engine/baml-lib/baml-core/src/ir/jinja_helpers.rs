use std::collections::HashMap;

use baml_types::{BamlValue, JinjaExpression};
use minijinja::value::{Kwargs, Value};
use regex::Regex;
use serde::Deserialize;

pub fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();

    env.set_formatter(|output, state, value| {
        // Top level (non-nested) none value is handled here.
        // Nested none values are handled in std::fmt::Display impl for
        // MinijinjaBamlClass and MinijinjaBamlList.
        // File is jinja-runtime/src/baml_value_to_jinja_value.rs.
        //
        // This is a little confusing and would be nice to replace all nones
        // with nulls in a single place but had no luck getting it to work,
        // this commit has commented code that attempts to do so:
        // https://github.com/BoundaryML/baml/pull/2037/commits/6facd2805f79be3e2000dc61058e772d9c5943be
        let value = if value.is_none() {
            &Value::from("null")
        } else {
            value
        };

        minijinja::escape_formatter(output, state, value)
    });

    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.add_filter("regex_match", regex_match);
    env.add_filter("sum", sum_filter);
    env.add_filter("toon", toon_filter);
    env
}

fn regex_match(value: String, regex: String) -> bool {
    match Regex::new(&regex) {
        Err(_) => false,
        Ok(re) => re.is_match(&value),
    }
}

fn sum_filter(value: Vec<Value>) -> Value {
    let int_sum: Option<i64> = value
        .iter()
        .map(|v| <i64>::try_from(v.clone()).ok())
        .collect::<Option<Vec<_>>>()
        .map(|ints| ints.into_iter().sum());
    let float_sum: Option<f64> = value
        .into_iter()
        .map(|v| <f64>::try_from(v).ok())
        .collect::<Option<Vec<_>>>()
        .map(|floats| floats.into_iter().sum());
    // If we could downcast all the Values to ints, return an int.
    // Otherwise, if we could downcast all the Values to floats, return the
    // float.
    // Otherwise, return 0. We rely on our jinja typechecker to make sure an
    // erroneous 0 never makes it back to the user.
    if int_sum.is_none() && float_sum.is_none() {
        log::warn!("The `sum` jinja filter was run against non-numeric arguments")
    }
    int_sum.map_or(float_sum.map_or(Value::from(0), Value::from), Value::from)
}

/// Convert a minijinja::Value to serde_json::Value for TOON encoding
fn minijinja_to_json(value: &Value) -> Result<serde_json::Value, minijinja::Error> {
    <serde_json::Value as Deserialize>::deserialize(value.clone()).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::BadSerialization,
            format!("Cannot convert value to JSON for TOON encoding: {}", e),
        )
    })
}

/// Parse TOON encoding options from Jinja kwargs
fn parse_toon_options(kwargs: Kwargs) -> Result<toon::EncodeOptions, minijinja::Error> {
    let mut options = toon::EncodeOptions::default();

    if let Ok(indent) = kwargs.get::<usize>("indent") {
        options.indent = indent;
    }

    if let Ok(delimiter_str) = kwargs.get::<String>("delimiter") {
        options.delimiter = match delimiter_str.as_str() {
            "comma" => toon::Delimiter::Comma,
            "tab" => toon::Delimiter::Tab,
            "pipe" => toon::Delimiter::Pipe,
            _ => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!(
                        "Invalid delimiter '{}'. Use 'comma', 'tab', or 'pipe'",
                        delimiter_str
                    ),
                ))
            }
        };
    }

    if let Ok(marker) = kwargs.get::<String>("length_marker") {
        if marker.len() == 1 {
            options.length_marker = Some(marker.chars().next().unwrap());
        } else {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("length_marker must be a single character, got '{}'", marker),
            ));
        }
    }

    Ok(options)
}

/// Jinja filter that converts a value to TOON (Token-Oriented Object Notation) format
fn toon_filter(value: Value, kwargs: Kwargs) -> Result<String, minijinja::Error> {
    // Convert minijinja::Value to serde_json::Value
    let json_value = minijinja_to_json(&value)?;

    // Parse encoding options from kwargs
    let options = parse_toon_options(kwargs)?;

    // Encode to TOON format
    Ok(toon::encode(&json_value, Some(options)))
}

/// Render a bare minijinaja expression with the given context.
/// E.g. `"a|length > 2"` with context `{"a": [1, 2, 3]}` will return `"true"`.
pub fn render_expression(
    expression: &JinjaExpression,
    ctx: &HashMap<String, minijinja::Value>,
) -> anyhow::Result<String> {
    let env = get_env();
    // In rust string literals, `{` is escaped as `{{`.
    // So producing the string `{{}}` requires writing the literal `"{{{{}}}}"`
    let template = format!(r#"{{{{ {} }}}}"#, expression.0);
    let args_dict = minijinja::Value::from_serialize(ctx);
    Ok(env.render_str(&template, &args_dict)?)
}

// TODO: (Greg) better error handling.
// TODO: (Greg) Upstream, typecheck the expression.
pub fn evaluate_predicate(
    this: &BamlValue,
    predicate_expression: &JinjaExpression,
) -> Result<bool, anyhow::Error> {
    let ctx: HashMap<String, minijinja::Value> =
        HashMap::from([("this".to_string(), minijinja::Value::from_serialize(this))]);
    match render_expression(predicate_expression, &ctx)?.as_ref() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(anyhow::anyhow!("Predicate did not evaluate to a boolean")),
    }
}

#[cfg(test)]
mod tests {
    use baml_types::BamlValue;

    use super::*;

    #[test]
    fn test_render_expressions() {
        let ctx = vec![
            (
                "a".to_string(),
                BamlValue::List(vec![
                    BamlValue::Int(1),
                    BamlValue::Int(2),
                    BamlValue::Int(3),
                ])
                .into(),
            ),
            (
                "b".to_string(),
                BamlValue::String("(123)456-7890".to_string()).into(),
            ),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            render_expression(&JinjaExpression("1".to_string()), &ctx).unwrap(),
            "1"
        );
        assert_eq!(
            render_expression(&JinjaExpression("1 + 1".to_string()), &ctx).unwrap(),
            "2"
        );
        assert_eq!(
            render_expression(&JinjaExpression("a|length > 2".to_string()), &ctx).unwrap(),
            "true"
        );
    }

    #[test]
    fn test_render_regex_match() {
        let ctx = vec![
            (
                "a".to_string(),
                BamlValue::List(vec![
                    BamlValue::Int(1),
                    BamlValue::Int(2),
                    BamlValue::Int(3),
                ])
                .into(),
            ),
            (
                "b".to_string(),
                BamlValue::String("(123)456-7890".to_string()).into(),
            ),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            render_expression(
                &JinjaExpression(r##"b|regex_match("123")"##.to_string()),
                &ctx
            )
            .unwrap(),
            "true"
        );
        assert_eq!(
            render_expression(
                &JinjaExpression(
                    r##"b|regex_match("\\(?\\d{3}\\)?[-.\\s]?\\d{3}[-.\\s]?\\d{4}")"##.to_string()
                ),
                &ctx
            )
            .unwrap(),
            "true"
        )
    }

    #[test]
    fn test_sum_filter() {
        let ctx = vec![].into_iter().collect();
        assert_eq!(
            render_expression(&JinjaExpression(r#"[1,2]|sum"#.to_string()), &ctx).unwrap(),
            "3"
        );

        assert_eq!(
            render_expression(&JinjaExpression(r#"[1,2.5]|sum"#.to_string()), &ctx).unwrap(),
            "3.5"
        );
    }

    /// Helper to test that the Jinja filter produces the same output as native TOON
    fn assert_toon_matches(
        json_value: serde_json::Value,
        jinja_expr: &str,
        toon_options: Option<toon::EncodeOptions>,
    ) {
        // Create context with the value
        let ctx = HashMap::from([(
            "data".to_string(),
            minijinja::Value::from_serialize(&json_value),
        )]);

        // Render via Jinja filter
        let jinja_result =
            render_expression(&JinjaExpression(jinja_expr.to_string()), &ctx).unwrap();

        // Encode directly with TOON library
        let toon_result = toon::encode(&json_value, toon_options);

        // They should match exactly
        assert_eq!(jinja_result, toon_result);
    }

    #[test]
    fn test_toon_filter_basic_object() {
        let json_value = serde_json::json!({
            "id": 123,
            "name": "Alice",
            "active": true
        });

        assert_toon_matches(json_value, "data|toon", None);
    }

    #[test]
    fn test_toon_filter_array_of_objects() {
        let json_value = serde_json::json!([
            {"id": 1, "name": "Alice", "role": "admin"},
            {"id": 2, "name": "Bob", "role": "user"}
        ]);

        assert_toon_matches(json_value, "data|toon", None);
    }

    #[test]
    fn test_toon_filter_nested_structure() {
        let json_value = serde_json::json!({
            "users": [
                {"id": 1, "name": "Alice"},
                {"id": 2, "name": "Bob"}
            ],
            "metadata": {
                "count": 2,
                "timestamp": "2025-01-01"
            }
        });

        assert_toon_matches(json_value, "data|toon", None);
    }

    #[test]
    fn test_toon_filter_with_indent_option() {
        let json_value = serde_json::json!({
            "nested": {
                "key": "value"
            }
        });

        let mut options = toon::EncodeOptions::default();
        options.indent = 4;

        assert_toon_matches(json_value, "data|toon(indent=4)", Some(options));
    }

    #[test]
    fn test_toon_filter_with_delimiter_comma() {
        let json_value = serde_json::json!(["a", "b", "c"]);

        let mut options = toon::EncodeOptions::default();
        options.delimiter = toon::Delimiter::Comma;

        assert_toon_matches(json_value, "data|toon(delimiter='comma')", Some(options));
    }

    #[test]
    fn test_toon_filter_with_delimiter_tab() {
        let json_value = serde_json::json!(["a", "b", "c"]);

        let mut options = toon::EncodeOptions::default();
        options.delimiter = toon::Delimiter::Tab;

        assert_toon_matches(json_value, "data|toon(delimiter='tab')", Some(options));
    }

    #[test]
    fn test_toon_filter_with_delimiter_pipe() {
        let json_value = serde_json::json!(["a", "b", "c"]);

        let mut options = toon::EncodeOptions::default();
        options.delimiter = toon::Delimiter::Pipe;

        assert_toon_matches(json_value, "data|toon(delimiter='pipe')", Some(options));
    }

    #[test]
    fn test_toon_filter_with_length_marker() {
        let json_value = serde_json::json!(["x", "y", "z"]);

        let mut options = toon::EncodeOptions::default();
        options.length_marker = Some('#');

        assert_toon_matches(json_value, "data|toon(length_marker='#')", Some(options));
    }

    #[test]
    fn test_toon_filter_with_all_options() {
        let json_value = serde_json::json!({
            "items": [
                {"id": 1, "name": "Item 1"},
                {"id": 2, "name": "Item 2"}
            ]
        });

        let mut options = toon::EncodeOptions::default();
        options.indent = 4;
        options.delimiter = toon::Delimiter::Pipe;
        options.length_marker = Some('#');

        assert_toon_matches(
            json_value,
            "data|toon(indent=4, delimiter='pipe', length_marker='#')",
            Some(options),
        );
    }

    #[test]
    fn test_toon_filter_primitives() {
        // Test various primitive types
        assert_toon_matches(serde_json::json!("string value"), "data|toon", None);

        assert_toon_matches(serde_json::json!(42), "data|toon", None);

        assert_toon_matches(serde_json::json!(3.14), "data|toon", None);

        assert_toon_matches(serde_json::json!(true), "data|toon", None);

        assert_toon_matches(serde_json::json!(null), "data|toon", None);
    }

    #[test]
    fn test_toon_filter_empty_structures() {
        assert_toon_matches(serde_json::json!([]), "data|toon", None);

        assert_toon_matches(serde_json::json!({}), "data|toon", None);
    }

    #[test]
    fn test_toon_filter_invalid_delimiter() {
        let ctx = HashMap::from([(
            "data".to_string(),
            minijinja::Value::from_serialize(&serde_json::json!({"key": "value"})),
        )]);

        let result = render_expression(
            &JinjaExpression("data|toon(delimiter='invalid')".to_string()),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid delimiter"));
    }

    #[test]
    fn test_toon_filter_multi_char_length_marker() {
        let ctx = HashMap::from([(
            "data".to_string(),
            minijinja::Value::from_serialize(&serde_json::json!(["a", "b"])),
        )]);

        let result = render_expression(
            &JinjaExpression("data|toon(length_marker='##')".to_string()),
            &ctx,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("single character"));
    }

    #[test]
    fn test_toon_filter_mixed_array() {
        let json_value = serde_json::json!([
            1,
            "string",
            true,
            {"key": "value"},
            [1, 2, 3]
        ]);

        assert_toon_matches(json_value, "data|toon", None);
    }

    #[test]
    fn test_toon_filter_deeply_nested() {
        let json_value = serde_json::json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "value": "deep"
                        }
                    }
                }
            }
        });

        assert_toon_matches(json_value, "data|toon", None);
    }
}
