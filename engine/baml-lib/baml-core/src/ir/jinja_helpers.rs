use std::collections::HashMap;

use baml_types::{BamlValue, JinjaExpression};
use regex::Regex;

pub fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.add_filter("regex_match", regex_match);
    env
}

fn regex_match(value: String, regex: String) -> bool {
    match Regex::new(&regex) {
        Err(_) => false,
        Ok(re) => re.is_match(&value)
    }
}

/// Render a bare minijinaja expression with the given context.
/// E.g. `"a|length > 2"` with context `{"a": [1, 2, 3]}` will return `"true"`.
pub fn render_expression(
    expression: &JinjaExpression,
    ctx: &HashMap<String, BamlValue>,
) -> anyhow::Result<String> {
    let env = get_env();
    // In rust string literals, `{` is escaped as `{{`.
    // So producing the string `{{}}` requires writing the literal `"{{{{}}}}"`
    let template = format!(r#"{{{{ {} }}}}"#, expression.0);
    let args_dict = minijinja::Value::from_serialize(ctx);
    eprintln!("{}", &template);
    Ok(env.render_str(&template, &args_dict)?)
}

// TODO: (Greg) better error handling.
// TODO: (Greg) Upstream, typecheck the expression.
pub fn evaluate_predicate(
    this: &BamlValue,
    predicate_expression: &JinjaExpression,
) -> Result<bool, anyhow::Error> {
    let ctx: HashMap<String, BamlValue> =
        [("this".to_string(), this.clone())].into_iter().collect();
    match render_expression(&predicate_expression, &ctx)?.as_ref() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(anyhow::anyhow!("TODO")),
    }
}

#[cfg(test)]
mod tests {
    use baml_types::BamlValue;
    use super::*;


    #[test]
    fn test_render_expressions() {
        let ctx = vec![(
            "a".to_string(),
            BamlValue::List(vec![BamlValue::Int(1), BamlValue::Int(2), BamlValue::Int(3)].into())
        ), ("b".to_string(), BamlValue::String("(123)456-7890".to_string()))]
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
        let ctx = vec![(
            "a".to_string(),
            BamlValue::List(vec![BamlValue::Int(1), BamlValue::Int(2), BamlValue::Int(3)].into())
        ), ("b".to_string(), BamlValue::String("(123)456-7890".to_string()))]
        .into_iter()
        .collect();
        assert_eq!(
            render_expression(&JinjaExpression(r##"b|regex_match("123")"##.to_string()), &ctx).unwrap(),
            "true"
        );
        assert_eq!(
            render_expression(&JinjaExpression(r##"b|regex_match("\\(?\\d{3}\\)?[-.\\s]?\\d{3}[-.\\s]?\\d{4}")"##.to_string()), &ctx).unwrap(),
            "true"
        )
    }
}
