mod evaluate_type;
mod get_vars;

use evaluate_type::get_variable_types;
use minijinja;
use serde_json;

pub use evaluate_type::{PredefinedTypes, Type, TypeError};

fn get_env<'a>() -> minijinja::Environment<'a> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env
}

#[derive(Debug)]
pub struct ValidationError {
    pub errors: Vec<TypeError>,
    pub parsing_errors: Option<minijinja::Error>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for err in &self.errors {
            writeln!(f, "{}", err)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub fn validate_template(
    name: &str,
    template: &str,
    types: &mut PredefinedTypes,
) -> Result<(), ValidationError> {
    let parsed =
        match minijinja::machinery::parse(template, name, Default::default(), Default::default()) {
            Ok(parsed) => parsed,
            Err(err) => {
                return Err(ValidationError {
                    errors: vec![],
                    parsing_errors: Some(err),
                });
            }
        };

    let errs = get_variable_types(&parsed, types);

    if errs.is_empty() {
        Ok(())
    } else {
        Err(ValidationError {
            errors: errs,
            parsing_errors: None,
        })
    }
}

fn render_minijinja(template: &str, json: &serde_json::Value) -> Result<String, minijinja::Error> {
    let mut env = get_env();

    env.add_template("prompt", template)?;
    let tmpl = env.get_template("prompt")?;

    tmpl.render(minijinja::Value::from_serializable(&json))
}

#[derive(Debug, PartialEq)]
pub struct RenderedChatMessage {
    pub role: String,
    pub message: String,
}

#[derive(Debug, PartialEq)]
pub enum RenderedPrompt {
    Completion(String),
    Chat(Vec<RenderedChatMessage>),
}

pub fn render_template(template: &str, json: &serde_json::Value) -> anyhow::Result<RenderedPrompt> {
    let rendered = render_minijinja(template, json);

    match rendered {
        Ok(s) => Ok(RenderedPrompt::Completion(s)),
        // Ok(s) => Ok(RenderedPrompt::Chat(vec![RenderedChatMessage {
        //     role: "system".to_string(),
        //     message: s,
        // }])),
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

    use super::*;

    use env_logger;
    use std::sync::Once;

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

        assert_eq!(rendered, RenderedPrompt::Completion("Hello, world!".into()));

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
