use std::collections::HashSet;

use minijinja::{Environment, value::Value};

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct JinjaError(#[from] minijinja::Error);

/// Create the `MiniJinja` environment used by BAML prompt rendering.
///
/// Runtime rendering and editor diagnostics both go through this function so
/// filters, syntax options, pycompat behavior, and formatter behavior do not
/// drift apart.
pub fn create_environment() -> Environment<'static> {
    let mut env = Environment::new();

    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    env.add_filter("regex_match", regex_match);
    env.add_filter("sum", sum);

    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);

    env.set_formatter(|out, _state, value| {
        if value.is_none() || value.is_undefined() {
            write!(out, "null").map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::WriteFailure, e.to_string())
            })
        } else {
            write!(out, "{value}").map_err(|e| {
                minijinja::Error::new(minijinja::ErrorKind::WriteFailure, e.to_string())
            })
        }
    });

    env
}

/// Return root names `MiniJinja` would need from the render context for a prompt.
///
/// `MiniJinja` does not subtract environment globals during this analysis, so
/// callers still decide which BAML names are in scope.
pub fn undeclared_prompt_variables(template: &str) -> Result<HashSet<String>, JinjaError> {
    let mut env = create_environment();
    env.add_template("prompt", template)?;
    Ok(env.get_template("prompt")?.undeclared_variables(false))
}

/// Filter: `regex_match` - Returns true if value matches regex pattern.
fn regex_match(value: &str, pattern: &str) -> bool {
    regex::Regex::new(pattern)
        .map(|re| re.is_match(value))
        .unwrap_or(false)
}

/// Filter: sum - Sum numeric values in a list.
#[allow(clippy::cast_precision_loss)]
fn sum(values: Vec<Value>) -> Value {
    let mut int_sum: i64 = 0;
    let mut float_sum: f64 = 0.0;
    let mut has_float = false;

    for val in values {
        if let Ok(i) = i64::try_from(val.clone()) {
            int_sum += i;
        } else if let Ok(f) = f64::try_from(val) {
            float_sum += f;
            has_float = true;
        }
    }

    if has_float {
        Value::from(int_sum as f64 + float_sum)
    } else {
        Value::from(int_sum)
    }
}

#[cfg(test)]
mod tests {
    use super::undeclared_prompt_variables;

    #[test]
    fn undeclared_prompt_variables_uses_runtime_jinja_semantics() {
        let vars = undeclared_prompt_variables(
            r#"
            {{ _.role("user") }}
            {{ name | lower }}
            {% for item in items %}
                {{ item.title }}
            {% endfor %}
            {{ ctx.output_format }}
            "#,
        )
        .unwrap();

        assert!(vars.contains("_"));
        assert!(vars.contains("ctx"));
        assert!(vars.contains("name"));
        assert!(vars.contains("items"));
        assert!(!vars.contains("item"));
        assert!(!vars.contains("lower"));
    }
}
