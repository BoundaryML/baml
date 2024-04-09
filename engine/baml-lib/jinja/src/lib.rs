mod evaluate_type;
mod get_vars;

use evaluate_type::get_variable_types;
use minijinja;
use minijinja::context;
use serde::Serialize;
use serde_json;
use std::collections::HashMap;

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

#[derive(Clone, Debug, Serialize)]
pub struct RenderContext_Client {
    pub name: String,
    pub provider: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RenderContext {
    pub client: RenderContext_Client,
    pub output_schema: String,
    pub env: HashMap<String, String>,
}

pub struct TemplateStringMacro {
    pub name: String,
    pub args: Vec<(String, String)>,
    pub template: String,
}

fn render_minijinja(
    template: &str,
    args: serde_json::Map<String, serde_json::Value>,
    ctx: RenderContext,
    template_string_macros: Vec<TemplateStringMacro>,
) -> Result<RenderedPrompt, minijinja::Error> {
    let mut env = get_env();

    // dedent
    let whitespace_length = template
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);
    let template = template
        .split('\n')
        .map(|line| line.chars().skip(whitespace_length).collect::<String>())
        .collect::<Vec<String>>()
        .join("\n");

    // trim
    let template = template.trim();

    // inject macros
    let template = template_string_macros
        .into_iter()
        .map(|tsm| {
            format!(
                "{{% macro {name}({args}) %}}{template}{{% endmacro %}}",
                name = tsm.name,
                args = tsm
                    .args
                    .into_iter()
                    .map(|(name, _type)| name)
                    .collect::<Vec<String>>()
                    .join(", "),
                template = tsm.template,
            )
        })
        .chain(std::iter::once(template.to_string()))
        .collect::<Vec<_>>()
        .join("\n");

    env.add_template("prompt", &template)?;
    env.add_global("ctx", minijinja::Value::from_serializable(&ctx));
    env.add_global(
        "_",
        context! {
            chat => minijinja::Value::from_function(|role: String| {
                format!("BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER:start:{role}:end:BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER")
            })
        },
    );
    let tmpl = env.get_template("prompt")?;

    let rendered = tmpl.render(minijinja::Value::from_serializable(&args))?;

    if !rendered.contains("BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER") {
        return Ok(RenderedPrompt::Completion(rendered));
    }

    let mut chat_messages = vec![];
    let mut role = "system";

    for chunk in rendered
        .trim()
        .split("BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER")
    {
        if chunk.starts_with(":start:") && chunk.ends_with(":end:") {
            role = chunk
                .strip_prefix(":start:")
                .unwrap_or(chunk)
                .strip_suffix(":end:")
                .unwrap_or(chunk);
        } else {
            chat_messages.push(RenderedChatMessage {
                role: role.to_string(),
                message: chunk.trim().to_string(),
            });
        }
    }

    Ok(RenderedPrompt::Chat(chat_messages))
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

pub fn render_template(
    template: &str,
    args: serde_json::Map<String, serde_json::Value>,
    ctx: RenderContext,
    template_string_macros: Vec<TemplateStringMacro>,
) -> anyhow::Result<RenderedPrompt> {
    let rendered = render_minijinja(template, args, ctx, template_string_macros);

    match rendered {
        Ok(r) => Ok(r),
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
mod render_tests {

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

        let serde_json::Value::Object(args) = serde_json::json!({
            "haiku_subject": "sakura"
        }) else {
            anyhow::bail!("args must be convertible to a JSON object");
        };

        let rendered = render_template(
            "
                    

                    You are an assistant that always responds
                    in a very excited way with emojis
                    and also outputs this word 4 times
                    after giving a response: {{ haiku_subject }}
                    
                    {{ _.chat(ctx.env.ROLE) }}
                    
                    Tell me a haiku about {{ haiku_subject }} in {{ ctx.output_schema }}.

                    End the haiku with a line about your maker, {{ ctx.client.provider }}.
            
            ",
            args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_schema: "iambic pentameter".to_string(),
                env: HashMap::from([("ROLE".to_string(), "john doe".to_string())]),
            },
            vec![],
        )?;

        assert_eq!(
            rendered,
            RenderedPrompt::Chat(vec![
                RenderedChatMessage {
                    role: "system".to_string(),
                    message: vec![
                        "You are an assistant that always responds",
                        "in a very excited way with emojis",
                        "and also outputs this word 4 times",
                        "after giving a response: sakura",
                    ]
                    .join("\n")
                },
                RenderedChatMessage {
                    role: "john doe".to_string(),
                    message: vec![
                        "Tell me a haiku about sakura in iambic pentameter.",
                        "",
                        "End the haiku with a line about your maker, openai.",
                    ]
                    .join("\n")
                }
            ])
        );

        Ok(())
    }

    #[test]
    fn rendering_fails() -> anyhow::Result<()> {
        setup_logging();

        let serde_json::Value::Object(args) = serde_json::json!({
            "name": "world"
        }) else {
            anyhow::bail!("args must be convertible to a JSON object");
        };

        // rendering should fail: template contains '{{ name }' (missing '}' at the end)
        let rendered = render_template(
            "Hello, {{ name }!",
            args,
            RenderContext {
                client: RenderContext_Client {
                    name: "gpt4".to_string(),
                    provider: "openai".to_string(),
                },
                output_schema: "output[]".to_string(),
                env: HashMap::new(),
            },
            vec![],
        );

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
