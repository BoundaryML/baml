use std::collections::HashMap;

use bex_external_types::{BexExternalValue, PromptAst as ExternalPromptAst};
use bex_llm_types::OutputFormatContent;
use indexmap::IndexMap;
use minijinja::Environment;

use crate::{
    MAGIC_CHAT_ROLE_DELIMITER, MAGIC_MEDIA_DELIMITER, filters,
    output_format_object::OutputFormatObject,
    value_conversion::{MediaTable, external_value_to_jinja},
};

/// Enum variant for Jinja rendering.
#[derive(Clone, Debug)]
pub struct RenderEnumVariant {
    pub name: String,
}

/// Enum definition for Jinja rendering.
#[derive(Clone, Debug)]
pub struct RenderEnum {
    pub name: String,
    pub variants: Vec<RenderEnumVariant>,
}

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
    /// Enum definitions available in templates.
    /// Each enum is accessible as a global, e.g., `{{ MyEnum.VALUE }}`.
    pub enums: HashMap<String, RenderEnum>,
}

// ============================================================================
// Jinja-internal PromptAst (private to this crate)
// ============================================================================

/// Jinja-internal PromptAst. Media is represented as a usize index into a lookup table
/// so it can survive minijinja template rendering (which requires string-serializable values).
#[derive(Clone, Debug, PartialEq)]
enum JinjaPromptAst {
    String(String),
    Media(usize),
    Message {
        role: String,
        content: Box<JinjaPromptAst>,
    },
    Vec(Vec<JinjaPromptAst>),
}

// ============================================================================
// Public API
// ============================================================================

/// Render a Jinja template to an external `PromptAst`.
///
/// # Arguments
/// * `template` - The Jinja template string
/// * `args` - Template arguments as pre-extracted `BexExternalValue` (no heap access needed)
/// * `ctx` - Rendering context with client info and output format
///
/// # Returns
/// A `bex_external_types::PromptAst` representing the rendered prompt.
pub fn render_prompt(
    template: &str,
    args: &IndexMap<String, BexExternalValue>,
    ctx: &RenderContext,
) -> Result<ExternalPromptAst, crate::RenderPromptError> {
    let mut env = create_environment();

    // Preprocess template
    let processed_template = preprocess_template(template);
    env.add_template("prompt", &processed_template)?;

    // Build media lookup table during value conversion
    let mut media_table: MediaTable = Vec::new();

    // Add globals (with media table for tag conversion)
    add_globals(&mut env, ctx, &mut media_table);

    // Build context - args are already extracted BexExternalValue
    let jinja_args: minijinja::value::Value = args
        .iter()
        .map(|(k, v)| (k.clone(), external_value_to_jinja(v, &mut media_table)))
        .collect();
    let tmpl = env.get_template("prompt")?;

    // Render
    let rendered = tmpl.render(jinja_args)?;

    // Parse to jinja-internal PromptAst (uses usize for Media)
    let jinja_ast = parse_rendered_output(&rendered, ctx);

    // Convert jinja-internal → external PromptAst using lookup table
    Ok(jinja_to_external_prompt_ast(jinja_ast, &media_table))
}

// ============================================================================
// Environment Setup
// ============================================================================

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

fn add_globals(env: &mut Environment, ctx: &RenderContext, media_table: &mut MediaTable) {
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

    // Build enums map - each enum is accessible as {{ ctx.enums.EnumName.VARIANT }}
    let enums_map: minijinja::value::Value = ctx
        .enums
        .iter()
        .map(|(name, def)| {
            let variants: IndexMap<String, minijinja::value::Value> = def
                .variants
                .iter()
                .map(|v| {
                    (
                        v.name.clone(),
                        minijinja::value::Value::from(v.name.clone()),
                    )
                })
                .collect();
            (name.clone(), minijinja::value::Value::from_iter(variants))
        })
        .collect();

    // Add ctx namespace with output_format and enums
    // Ported from engine/baml-lib/jinja-runtime/src/output_format/mod.rs
    let output_format = OutputFormatObject::new(ctx.output_format.clone());
    env.add_global(
        "ctx",
        context! {
            client => context! {
                name => ctx.client.name.clone(),
                provider => ctx.client.provider.clone(),
            },
            tags => ctx.tags.iter().map(|(k, v)| (k.clone(), external_value_to_jinja(v, media_table))).collect::<minijinja::value::Value>(),
            output_format => minijinja::value::Value::from_object(output_format),
            enums => enums_map,
        },
    );
}

// ============================================================================
// Jinja-internal Parsing
// ============================================================================

fn parse_rendered_output(rendered: &str, ctx: &RenderContext) -> JinjaPromptAst {
    // Check if this is a chat-style prompt (contains role delimiters)
    if rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) {
        parse_chat_prompt(rendered, ctx)
    } else {
        // Simple completion prompt
        JinjaPromptAst::String(rendered.to_string())
    }
}

fn parse_chat_prompt(rendered: &str, _ctx: &RenderContext) -> JinjaPromptAst {
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
                messages.push(JinjaPromptAst::Message {
                    role,
                    content: Box::new(content),
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
        messages.push(JinjaPromptAst::Message {
            role,
            content: Box::new(content),
        });
    }

    if messages.is_empty() {
        JinjaPromptAst::String(rendered.to_string())
    } else if messages.len() == 1 {
        messages.pop().unwrap()
    } else {
        JinjaPromptAst::Vec(messages)
    }
}

fn parse_message_content(content: &str) -> JinjaPromptAst {
    // Check for media delimiters
    if content.contains(MAGIC_MEDIA_DELIMITER) {
        let mut parts = Vec::new();
        let chunks: Vec<&str> = content.split(MAGIC_MEDIA_DELIMITER).collect();

        for (i, chunk) in chunks.iter().enumerate() {
            if i % 2 == 1 {
                // This is a media chunk - parse the handle
                // Format: :baml-start-media:{handle}:baml-end-media:
                if let Some(handle) = parse_media_handle(chunk) {
                    parts.push(JinjaPromptAst::Media(handle));
                }
            } else if !chunk.trim().is_empty() {
                parts.push(JinjaPromptAst::String((*chunk).to_string()));
            }
        }

        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            JinjaPromptAst::Vec(parts)
        }
    } else {
        JinjaPromptAst::String(content.trim().to_string())
    }
}

fn parse_media_handle(chunk: &str) -> Option<usize> {
    // Extract handle from format: :baml-start-media:{handle}:baml-end-media:
    chunk
        .strip_prefix(":baml-start-media:")
        .and_then(|s| s.strip_suffix(":baml-end-media:"))
        .and_then(|s| s.parse().ok())
}

// ============================================================================
// Jinja-internal → External conversion
// ============================================================================

/// Convert jinja-internal PromptAst to VM-external PromptAst using the media lookup table.
fn jinja_to_external_prompt_ast(
    ast: JinjaPromptAst,
    media_table: &[(bex_external_types::Handle, baml_base::MediaKind)],
) -> ExternalPromptAst {
    match ast {
        JinjaPromptAst::String(s) => ExternalPromptAst::String(s),
        JinjaPromptAst::Media(index) => {
            let (handle, kind) = &media_table[index];
            ExternalPromptAst::Media {
                handle: handle.clone(),
                kind: *kind,
            }
        }
        JinjaPromptAst::Message { role, content } => ExternalPromptAst::Message {
            role,
            content: Box::new(jinja_to_external_prompt_ast(*content, media_table)),
            metadata: Box::new(BexExternalValue::Null),
        },
        JinjaPromptAst::Vec(items) => ExternalPromptAst::Vec(
            items
                .into_iter()
                .map(|item| jinja_to_external_prompt_ast(item, media_table))
                .collect(),
        ),
    }
}

// ============================================================================
// Tests
// ============================================================================

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
            enums: HashMap::new(),
        }
    }

    #[test]
    fn test_simple_string() {
        let template = "Hello, world!";
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(
            result,
            ExternalPromptAst::String("Hello, world!".to_string())
        );
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

        assert_eq!(
            result,
            ExternalPromptAst::String("Hello, Alice!".to_string())
        );
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

        assert_eq!(
            result,
            ExternalPromptAst::String("Name: Bob, Age: 30".to_string())
        );
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

        let expected = ExternalPromptAst::Vec(vec![
            ExternalPromptAst::Message {
                role: "system".to_string(),
                content: Box::new(ExternalPromptAst::String(
                    "You are a helpful assistant.".to_string(),
                )),
                metadata: Box::new(BexExternalValue::Null),
            },
            ExternalPromptAst::Message {
                role: "user".to_string(),
                content: Box::new(ExternalPromptAst::String("Hello!".to_string())),
                metadata: Box::new(BexExternalValue::Null),
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

        let expected = ExternalPromptAst::Message {
            role: "user".to_string(),
            content: Box::new(ExternalPromptAst::String(
                "Hello with default role!".to_string(),
            )),
            metadata: Box::new(BexExternalValue::Null),
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

        assert_eq!(
            result,
            ExternalPromptAst::String("Hello,\nWorld!".to_string())
        );
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
            ExternalPromptAst::String("Items: apple, banana, cherry".to_string())
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

        assert_eq!(
            result,
            ExternalPromptAst::String("Answer as an int".to_string())
        );
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
            ExternalPromptAst::String("Please respond with: int".to_string())
        );
    }

    #[test]
    fn test_enum_access() {
        let template = "Category: {{ ctx.enums.Category.SPORTS }}";
        let args = IndexMap::new();

        let mut ctx = test_ctx();
        ctx.enums.insert(
            "Category".to_string(),
            RenderEnum {
                name: "Category".to_string(),
                variants: vec![
                    RenderEnumVariant {
                        name: "SPORTS".to_string(),
                    },
                    RenderEnumVariant {
                        name: "TECH".to_string(),
                    },
                    RenderEnumVariant {
                        name: "POLITICS".to_string(),
                    },
                ],
            },
        );

        let result = render_prompt(template, &args, &ctx).unwrap();

        assert_eq!(
            result,
            ExternalPromptAst::String("Category: SPORTS".to_string())
        );
    }
}
