use anyhow;
use log;
use minijinja;
use serde_json;
use std::collections::HashMap;

// TODO:

struct PromptContext {
    client: PromptClient,
    output_schema: String,
    // we can also make env accessible top-level in the future
    env: HashMap<String, String>,
}

struct PromptClient {
    name: String,
    provider: String,
}

pub struct RenderError {
    error: minijinja::Error,
}

//impl std::error::Error for RenderError {}
/*

we want to inject
- ctx.client
- ctx.output_schema
- env.DEV_MODE (all envvars)
-

*/

fn render_minijinja(template: &str, json: &serde_json::Value) -> Result<String, minijinja::Error> {
    let mut env = minijinja::Environment::new();
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

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
