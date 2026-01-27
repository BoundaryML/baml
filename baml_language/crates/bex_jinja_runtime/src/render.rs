use bex_external_types::BexExternalValue;
use bex_llm_types::OutputFormatContent;
use bex_vm_types::{PromptAst, Value};
use indexmap::IndexMap;
use minijinja::Environment;

use crate::{
    MAGIC_CHAT_ROLE_DELIMITER, MAGIC_MEDIA_DELIMITER, filters,
    output_format_object::OutputFormatObject, value_conversion::external_value_to_jinja,
};

/// Client configuration for rendering.
#[derive(Clone, Debug)]
pub struct RenderContextClient {
    pub name: String,
    pub provider: String,
    pub default_role: String,
    pub allowed_roles: Vec<String>,
}

/// Context for rendering a prompt.
#[derive(Clone, Debug)]
pub struct RenderContext {
    pub client: RenderContextClient,
    pub output_format: OutputFormatContent,
    pub tags: IndexMap<String, BexExternalValue>,
}

/// Render a Jinja template to a `PromptAst`.
///
/// # Arguments
/// * `template` - The Jinja template string
/// * `args` - Template arguments as pre-extracted `BexExternalValue` (no heap access needed)
/// * `ctx` - Rendering context with client info and output format
///
/// # Returns
/// A `PromptAst` representing the rendered prompt.
pub fn render_prompt(
    template: &str,
    args: &IndexMap<String, BexExternalValue>,
    ctx: &RenderContext,
) -> Result<PromptAst, minijinja::Error> {
    let mut env = create_environment();

    // Preprocess template
    let processed_template = preprocess_template(template);
    env.add_template("prompt", &processed_template)?;

    // Add globals
    add_globals(&mut env, ctx);

    // Build context - args are already extracted BexExternalValue
    let jinja_args: minijinja::value::Value = args
        .iter()
        .map(|(k, v)| (k.clone(), external_value_to_jinja(v)))
        .collect();
    let tmpl = env.get_template("prompt")?;

    // Render
    let rendered = tmpl.render(jinja_args)?;

    // Parse result into PromptAst
    Ok(parse_rendered_output(&rendered, ctx))
}

fn create_environment() -> Environment<'static> {
    let mut env = Environment::new();

    // Configure environment
    env.set_debug(true);
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);

    // Add filters
    env.add_filter("regex_match", filters::regex_match);
    env.add_filter("sum", filters::sum);

    // Custom formatter: replace 'none' with 'null'
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

/// Preprocess template: dedent and trim.
///
/// Dedenting logic ported from engine/baml-lib/jinja-runtime/src/lib.rs:266-277
fn preprocess_template(template: &str) -> String {
    // Dedent: find minimum whitespace and remove from all lines
    let lines: Vec<&str> = template.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn add_globals(env: &mut Environment, ctx: &RenderContext) {
    use minijinja::context;

    // Create role function - same function used for both _.role() and _.chat()
    // Ported from engine/baml-lib/jinja-runtime/src/lib.rs:382-387
    let default_role = ctx.client.default_role.clone();
    let role_fn = minijinja::value::Value::from_function(move |role: Option<String>| -> String {
        let r = role.unwrap_or_else(|| default_role.clone());
        format!(
            "{MAGIC_CHAT_ROLE_DELIMITER}:baml-start-role:{r}:baml-end-role:{MAGIC_CHAT_ROLE_DELIMITER}"
        )
    });

    // Add _ namespace with chat and role functions
    env.add_global(
        "_",
        context! {
            chat => role_fn,
            role => role_fn,
        },
    );

    // Add ctx namespace with output_format
    // Ported from engine/baml-lib/jinja-runtime/src/output_format/mod.rs
    let output_format = OutputFormatObject::new(ctx.output_format.clone());
    env.add_global(
        "ctx",
        context! {
            client => context! {
                name => ctx.client.name.clone(),
                provider => ctx.client.provider.clone(),
            },
            tags => ctx.tags.iter().map(|(k, v)| (k.clone(), external_value_to_jinja(v))).collect::<minijinja::value::Value>(),
            output_format => minijinja::value::Value::from_object(output_format),
        },
    );
}

fn parse_rendered_output(rendered: &str, ctx: &RenderContext) -> PromptAst {
    // Check if this is a chat-style prompt (contains role delimiters)
    if rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) {
        parse_chat_prompt(rendered, ctx)
    } else {
        // Simple completion prompt
        PromptAst::String(rendered.to_string())
    }
}

fn parse_chat_prompt(rendered: &str, _ctx: &RenderContext) -> PromptAst {
    let mut messages = Vec::new();

    // Split on role delimiter
    let parts: Vec<&str> = rendered.split(MAGIC_CHAT_ROLE_DELIMITER).collect();

    let mut current_role: Option<String> = None;
    let mut current_content = String::new();

    for part in parts {
        if part.starts_with(":baml-start-role:") && part.ends_with(":baml-end-role:") {
            // Save previous message if any
            if let Some(role) = current_role.take() {
                let content = parse_message_content(&current_content);
                messages.push(PromptAst::Message {
                    role,
                    content: Box::new(content),
                    metadata: Value::Null,
                });
                current_content.clear();
            }

            // Extract new role
            let role = part
                .strip_prefix(":baml-start-role:")
                .and_then(|s| s.strip_suffix(":baml-end-role:"))
                .unwrap_or("user")
                .to_string();
            current_role = Some(role);
        } else {
            current_content.push_str(part);
        }
    }

    // Save last message
    if let Some(role) = current_role {
        let content = parse_message_content(&current_content);
        messages.push(PromptAst::Message {
            role,
            content: Box::new(content),
            metadata: Value::Null,
        });
    }

    if messages.is_empty() {
        PromptAst::String(rendered.to_string())
    } else if messages.len() == 1 {
        messages.pop().unwrap()
    } else {
        PromptAst::Vec(messages)
    }
}

fn parse_message_content(content: &str) -> PromptAst {
    // Check for media delimiters
    if content.contains(MAGIC_MEDIA_DELIMITER) {
        let mut parts = Vec::new();
        let chunks: Vec<&str> = content.split(MAGIC_MEDIA_DELIMITER).collect();

        for (i, chunk) in chunks.iter().enumerate() {
            if i % 2 == 1 {
                // This is a media chunk - parse the handle
                // Format: :baml-start-media:{handle}:baml-end-media:
                if let Some(handle) = parse_media_handle(chunk) {
                    parts.push(PromptAst::Media(handle));
                }
            } else if !chunk.trim().is_empty() {
                parts.push(PromptAst::String((*chunk).to_string()));
            }
        }

        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            PromptAst::Vec(parts)
        }
    } else {
        PromptAst::String(content.trim().to_string())
    }
}

fn parse_media_handle(chunk: &str) -> Option<usize> {
    // Extract handle from format: :baml-start-media:{handle}:baml-end-media:
    chunk
        .strip_prefix(":baml-start-media:")
        .and_then(|s| s.strip_suffix(":baml-end-media:"))
        .and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use bex_program::Ty;

    use super::*;

    fn test_ctx() -> RenderContext {
        RenderContext {
            client: RenderContextClient {
                name: "test".to_string(),
                provider: "openai".to_string(),
                default_role: "user".to_string(),
                allowed_roles: vec![
                    "user".to_string(),
                    "assistant".to_string(),
                    "system".to_string(),
                ],
            },
            output_format: OutputFormatContent::new(Ty::String),
            tags: IndexMap::new(),
        }
    }

    #[test]
    fn test_simple_string() {
        let template = "Hello, world!";
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, PromptAst::String("Hello, world!".to_string()));
    }

    #[test]
    fn test_variable_substitution() {
        let template = "Hello, {{ name }}!";

        let mut args = IndexMap::new();
        args.insert(
            "name".to_string(),
            BexExternalValue::String("Alice".to_string()),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, PromptAst::String("Hello, Alice!".to_string()));
    }

    #[test]
    fn test_nested_object() {
        let template = "Name: {{ person.name }}, Age: {{ person.age }}";

        let mut person_fields = IndexMap::new();
        person_fields.insert(
            "name".to_string(),
            BexExternalValue::String("Bob".to_string()),
        );
        person_fields.insert("age".to_string(), BexExternalValue::Int(30));

        let mut args = IndexMap::new();
        args.insert(
            "person".to_string(),
            BexExternalValue::Instance {
                class_name: "Person".to_string(),
                fields: person_fields,
            },
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, PromptAst::String("Name: Bob, Age: 30".to_string()));
    }

    #[test]
    fn test_chat_with_roles() {
        let template = r#"
            {{ _.role("system") }}
            You are a helpful assistant.
            {{ _.role("user") }}
            Hello!
        "#;
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        let expected = PromptAst::Vec(vec![
            PromptAst::Message {
                role: "system".to_string(),
                content: Box::new(PromptAst::String(
                    "You are a helpful assistant.".to_string(),
                )),
                metadata: Value::Null,
            },
            PromptAst::Message {
                role: "user".to_string(),
                content: Box::new(PromptAst::String("Hello!".to_string())),
                metadata: Value::Null,
            },
        ]);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_chat_default_role() {
        let template = r#"
            {{ _.chat() }}
            Hello with default role!
        "#;
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        let expected = PromptAst::Message {
            role: "user".to_string(),
            content: Box::new(PromptAst::String("Hello with default role!".to_string())),
            metadata: Value::Null,
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_dedent() {
        let template = r#"
            Hello,
            World!
        "#;
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, PromptAst::String("Hello,\nWorld!".to_string()));
    }

    #[test]
    fn test_array_iteration() {
        let template = "Items: {% for item in items %}{{ item }}{% if not loop.last %}, {% endif %}{% endfor %}";

        let mut args = IndexMap::new();
        args.insert(
            "items".to_string(),
            BexExternalValue::Array {
                element_type: Ty::String,
                items: vec![
                    BexExternalValue::String("apple".to_string()),
                    BexExternalValue::String("banana".to_string()),
                    BexExternalValue::String("cherry".to_string()),
                ],
            },
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(
            result,
            PromptAst::String("Items: apple, banana, cherry".to_string())
        );
    }

    #[test]
    fn test_output_format_in_template() {
        let template = "{{ ctx.output_format }}";
        let args = IndexMap::new();

        // Create a context with an int output format
        let mut ctx = test_ctx();
        ctx.output_format = OutputFormatContent::new(Ty::Int);

        let result = render_prompt(template, &args, &ctx).unwrap();

        assert_eq!(result, PromptAst::String("Answer as an int".to_string()));
    }

    #[test]
    fn test_output_format_with_kwargs() {
        let template = "{{ ctx.output_format(prefix='Please respond with: ') }}";
        let args = IndexMap::new();

        let mut ctx = test_ctx();
        ctx.output_format = OutputFormatContent::new(Ty::Int);

        let result = render_prompt(template, &args, &ctx).unwrap();

        assert_eq!(
            result,
            PromptAst::String("Please respond with: int".to_string())
        );
    }
}
