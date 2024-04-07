mod evaluate_type;
mod get_vars;

use std::collections::HashMap;

use anyhow::Result;
use get_vars::Dependencies;
use minijinja;
use serde_json;

struct RenderError {
    error: minijinja::Error,
}

fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env
}

fn render_minijinja(template: &str, json: &serde_json::Value) -> Result<String, minijinja::Error> {
    let mut env = get_env();

    env.add_template("prompt", template)?;
    let tmpl = env.get_template("prompt")?;

    tmpl.render(minijinja::Value::from_serializable(&json))
}

pub fn render_template(template: &str, json: &serde_json::Value) -> anyhow::Result<String> {
    let rendered = render_minijinja(template, json);

    match rendered {
        Ok(s) => Ok(s),
        Err(err) => {
            let mut minijinja_err = "".to_string();
            minijinja_err += &format!("{err:#}");

            let mut err = &err as &dyn std::error::Error;
            while let Some(next_err) = err.source() {
                minijinja_err += &format!("\n\ncaused by: {next_err:#}");
                err = next_err;
            }

            anyhow::bail!("Error occurred while rendering prompt: {minijinja_err}");
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::get_vars::{FunctionCall, ParameterizedValue};

    use super::*;

    use env_logger;
    use std::{collections::BTreeMap, sync::Once, vec};

    static INIT: Once = Once::new();

    pub fn setup_logging() {
        INIT.call_once(|| {
            env_logger::init();
        });
    }

    #[test]
    fn rendering_succeeds() -> anyhow::Result<()> {
        setup_logging();

        let rendered =
            render_template("Hello, {{ name }}!", &serde_json::json!({"name": "world"}))?;

        assert_eq!(rendered, "Hello, world!");

        Ok(())
    }

    #[test]
    fn rendering_fails() -> anyhow::Result<()> {
        setup_logging();

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_template("Hello, {{ name }!", &serde_json::json!({"name": "world"}));

        match rendered {
            Ok(_) => {
                anyhow::bail!("Expected template rendering to fail, but it succeeded");
            }
            Err(e) => assert!(e
                .to_string()
                .contains("Error occurred while rendering prompt:")),
        }

        Ok(())
    }
}
