//! Jinja runtime for BAML.
//!
//! This crate provides:
//! - `render_prompt` - Render a Jinja template with BAML values
//! - `RenderedPrompt` - The result of rendering a prompt template

mod baml_value_to_jinja;

use std::collections::HashMap;

// Re-export LLM interface types
pub use baml_llm_interface::{ChatMessagePart, LlmClientSpec, RenderedChatMessage, RenderedPrompt};
pub use baml_value_to_jinja::IntoMiniJinjaValue;
use baml_value_to_jinja::MAGIC_MEDIA_DELIMITER;
use ir_stub::{BamlMedia, BamlValue};
use minijinja::{ErrorKind, context, value::Kwargs};
use serde::Deserialize;

const MAGIC_CHAT_ROLE_DELIMITER: &str = "BAML_CHAT_ROLE_MAGIC_STRING_DELIMITER";

// ============================================================================
// Render Context
// ============================================================================

/// Context for rendering a prompt.
#[derive(Debug)]
pub struct RenderContext {
    /// Client configuration.
    pub client: LlmClientSpec,
    /// Tags available in the template.
    pub tags: HashMap<String, BamlValue>,
}

impl Default for RenderContext {
    fn default() -> Self {
        Self {
            client: LlmClientSpec::default(),
            tags: HashMap::new(),
        }
    }
}

// ============================================================================
// Render Error
// ============================================================================

/// Error during template rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Minijinja error: {0}")]
    MiniJinja(#[from] minijinja::Error),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Render error: {0}")]
    Other(String),
}

// ============================================================================
// Render Functions
// ============================================================================

/// Render a prompt template with the given arguments and context.
///
/// This is the main entry point for rendering BAML prompts.
pub fn render_prompt(
    template: &str,
    args: &BamlValue,
    ctx: RenderContext,
) -> Result<RenderedPrompt, RenderError> {
    let default_role = ctx.client.default_role.clone();
    let allowed_roles = ctx.client.allowed_roles.clone();
    let remap_role = ctx.client.remap_role.clone();

    // Convert args to minijinja value
    let args_jinja = args.to_minijinja_value();

    render_minijinja(
        template,
        &args_jinja,
        ctx,
        default_role,
        allowed_roles,
        remap_role,
    )
}

fn render_minijinja(
    template: &str,
    args: &minijinja::Value,
    ctx: RenderContext,
    default_role: String,
    allowed_roles: Vec<String>,
    remap_role: HashMap<String, String>,
) -> Result<RenderedPrompt, RenderError> {
    let mut env = minijinja::Environment::new();

    // Allow undefined variables to render as empty string instead of erroring
    // This enables testing templates without providing all required arguments
    env.set_undefined_behavior(minijinja::UndefinedBehavior::Chainable);

    // Dedent the template
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
    let template = template.trim();

    env.add_template("prompt", template)?;

    // Add ctx global
    let client = ctx.client.clone();
    let tags = ctx.tags.clone();
    env.add_global(
        "ctx",
        context! {
            client => client,
            tags => tags,
        },
    );

    // Add the role function for _.chat() / _.role()
    let role_fn = minijinja::Value::from_function(
        |role: Option<String>, kwargs: Kwargs| -> Result<String, minijinja::Error> {
            let role = match (role, kwargs.get::<String>("role")) {
                (Some(b), Ok(a)) => {
                    return Err(minijinja::Error::new(
                        ErrorKind::TooManyArguments,
                        format!("role() called with two roles: '{a}' and '{b}'"),
                    ));
                }
                (Some(role), _) => role,
                (_, Ok(role)) => role,
                _ => {
                    return Err(minijinja::Error::new(
                        ErrorKind::MissingArgument,
                        "role() called without role. Try role('role') or role(role='role').",
                    ));
                }
            };

            let allow_duplicate_role = match kwargs.get::<bool>("__baml_allow_dupe_role__") {
                Ok(allow) => allow,
                Err(e) => match e.kind() {
                    ErrorKind::MissingArgument => false,
                    _ => return Err(e),
                },
            };

            let additional_properties = {
                let mut props = kwargs
                    .args()
                    .filter(|&k| k != "role")
                    .map(|k| {
                        Ok((
                            k,
                            serde_json::Value::deserialize(kwargs.get::<minijinja::Value>(k)?)?,
                        ))
                    })
                    .collect::<Result<HashMap<&str, serde_json::Value>, minijinja::Error>>()?;

                props.insert("role", role.clone().into());
                props.insert("__baml_allow_dupe_role__", allow_duplicate_role.into());
                props
            };

            let additional_properties = serde_json::json!(additional_properties).to_string();

            Ok(format!(
                "{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-baml:{additional_properties}:baml-end-baml:{MAGIC_CHAT_ROLE_DELIMITER}"
            ))
        },
    );

    env.add_global(
        "_",
        context! {
            chat => role_fn,
            role => role_fn
        },
    );

    let tmpl = env.get_template("prompt")?;
    let rendered = tmpl.render(args)?;

    // If no chat delimiters, return as completion
    if !rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) && !rendered.contains(MAGIC_MEDIA_DELIMITER) {
        return Ok(RenderedPrompt::Completion { text: rendered });
    }

    // Parse chat messages
    let mut chat_messages = vec![];
    let mut role = None;
    let mut meta = None;
    let mut allow_duplicate_role = false;

    for chunk in rendered.split(MAGIC_CHAT_ROLE_DELIMITER) {
        if chunk.starts_with(":baml-start-baml:") && chunk.ends_with(":baml-end-baml:") {
            let parsed = chunk
                .strip_prefix(":baml-start-baml:")
                .unwrap_or(chunk)
                .strip_suffix(":baml-end-baml:")
                .unwrap_or(chunk);

            if let Ok(mut parsed) =
                serde_json::from_str::<HashMap<String, serde_json::Value>>(parsed)
            {
                if let Some(role_val) = parsed.remove("role") {
                    role = Some(role_val.as_str().unwrap_or("").to_string());
                }

                allow_duplicate_role = parsed
                    .remove("__baml_allow_dupe_role__")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if parsed.is_empty() {
                    meta = None;
                } else {
                    meta = Some(parsed);
                }
            }
        } else if role.is_none() && chunk.is_empty() {
            // Discard whitespace before first _.chat()
        } else {
            let mut parts = vec![];

            for part in chunk.split(MAGIC_MEDIA_DELIMITER) {
                let part = if part.starts_with(":baml-start-media:")
                    && part.ends_with(":baml-end-media:")
                {
                    let media_data = part
                        .strip_prefix(":baml-start-media:")
                        .unwrap_or(part)
                        .strip_suffix(":baml-end-media:")
                        .unwrap_or(part);

                    match serde_json::from_str::<BamlMedia>(media_data) {
                        Ok(m) => Some(ChatMessagePart::Media { media: m }),
                        Err(_) => {
                            return Err(RenderError::Other(format!(
                                "Media variable had unrecognizable data: {media_data}"
                            )));
                        }
                    }
                } else if !part.trim().is_empty() {
                    Some(ChatMessagePart::Text { text: part.trim().to_string() })
                } else {
                    None
                };

                if let Some(part) = part {
                    if let Some(meta) = &meta {
                        parts.push(part.with_meta(meta.clone()));
                    } else {
                        parts.push(part);
                    }
                }
            }

            if !parts.is_empty() {
                chat_messages.push(RenderedChatMessage {
                    role: match role.as_ref() {
                        Some(r) if allowed_roles.contains(r) => r.clone(),
                        Some(_) => default_role.clone(),
                        None => default_role.clone(),
                    },
                    allow_duplicate_role,
                    parts,
                });
            }
        }
    }

    // Apply role remapping
    for msg in &mut chat_messages {
        if let Some(remap) = remap_role.get(&msg.role) {
            msg.role = remap.clone();
        }
    }

    Ok(RenderedPrompt::Chat { messages: chat_messages })
}

/// Evaluate a Jinja expression on a BamlValue.
///
/// Used for constraint evaluation.
pub fn evaluate_predicate(
    value: &BamlValue,
    expression: &ir_stub::JinjaExpression,
) -> Result<bool, RenderError> {
    let mut env = minijinja::Environment::new();

    // Create a template that evaluates the expression
    let template = format!(
        "{{% if {} %}}true{{% else %}}false{{% endif %}}",
        expression.0
    );
    env.add_template("predicate", &template)?;

    let tmpl = env.get_template("predicate")?;
    let jinja_value = value.to_minijinja_value();

    // Wrap the value as "this" for the expression
    let ctx = context! {
        this => jinja_value,
    };

    let result = tmpl.render(ctx)?;
    Ok(result.trim() == "true")
}

#[cfg(test)]
mod tests {
    use ir_stub::BamlMap;

    use super::*;

    #[test]
    fn test_render_prompt_completion() {
        let mut args = BamlMap::new();
        args.insert("text".to_string(), BamlValue::String("Hello".to_string()));
        let args = BamlValue::Map(args);

        let result = render_prompt("Process: {{ text }}", &args, RenderContext::default());

        assert!(result.is_ok());
        match result.unwrap() {
            RenderedPrompt::Completion { text } => assert_eq!(text, "Process: Hello"),
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn test_render_prompt_chat() {
        let mut args = BamlMap::new();
        args.insert("text".to_string(), BamlValue::String("Hello".to_string()));
        let args = BamlValue::Map(args);

        let template = r#"{{ _.chat("system") }}
You are a helpful assistant.
{{ _.chat("user") }}
{{ text }}"#;

        let result = render_prompt(template, &args, RenderContext::default());

        assert!(result.is_ok());
        match result.unwrap() {
            RenderedPrompt::Chat { messages } => {
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].role, "system");
                assert_eq!(messages[1].role, "user");
            }
            _ => panic!("Expected Chat"),
        }
    }

    #[test]
    fn test_render_prompt_with_list() {
        let args = BamlValue::Map({
            let mut m = BamlMap::new();
            m.insert(
                "items".to_string(),
                BamlValue::List(vec![
                    BamlValue::String("one".to_string()),
                    BamlValue::String("two".to_string()),
                ]),
            );
            m
        });

        let template = r#"{% for item in items %}- {{ item }}
{% endfor %}"#;

        let result = render_prompt(template, &args, RenderContext::default());
        assert!(result.is_ok());
        match result.unwrap() {
            RenderedPrompt::Completion { text } => {
                assert!(text.contains("- one"));
                assert!(text.contains("- two"));
            }
            _ => panic!("Expected Completion"),
        }
    }

    #[test]
    fn test_render_prompt_role_alias() {
        let args = BamlValue::Map(BamlMap::new());

        let template = r#"{{ _.role("system") }}
Hello"#;

        let result = render_prompt(template, &args, RenderContext::default());

        assert!(result.is_ok());
        match result.unwrap() {
            RenderedPrompt::Chat { messages } => {
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].role, "system");
            }
            _ => panic!("Expected Chat"),
        }
    }

    #[test]
    fn test_evaluate_predicate() {
        let value = BamlValue::Int(42);
        let expr = ir_stub::JinjaExpression("this > 10".to_string());

        let result = evaluate_predicate(&value, &expr);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_evaluate_predicate_false() {
        let value = BamlValue::Int(5);
        let expr = ir_stub::JinjaExpression("this > 10".to_string());

        let result = evaluate_predicate(&value, &expr);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_evaluate_predicate_string() {
        let value = BamlValue::String("hello world".to_string());
        let expr = ir_stub::JinjaExpression("this | length > 5".to_string());

        let result = evaluate_predicate(&value, &expr);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
