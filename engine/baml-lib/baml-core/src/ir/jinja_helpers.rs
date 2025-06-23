use std::collections::HashMap;

use baml_types::{BamlValue, JinjaExpression};
use minijinja::value::Value;
use regex::Regex;

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
}
