use std::collections::HashMap;

use baml_builtins2::{PromptAst, PromptAstSimple};
use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use minijinja::Environment;
use sys_jinja::create_environment;

use super::{
    MAGIC_CHAT_ROLE_DELIMITER, MAGIC_MEDIA_DELIMITER, output_format_object::OutputFormatObject,
    value_conversion::external_value_to_jinja,
};
use crate::types::OutputFormatContent;

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
) -> Result<PromptAst, super::RenderPromptError> {
    let mut env = create_environment();

    env.add_template("prompt", template)?;

    let mut media_handles = HashMap::new();
    // Add globals (tags may contain media; share media_handles so parse_rendered_output can resolve them)
    add_globals(&mut env, ctx, &mut media_handles)?;

    // Build context - args are already extracted BexExternalValue
    let jinja_args: minijinja::value::Value = args
        .iter()
        .map(|(k, v)| Ok((k.clone(), external_value_to_jinja(v, &mut media_handles)?)))
        .collect::<Result<_, super::RenderPromptError>>()?;
    let tmpl = env.get_template("prompt")?;

    // Render
    let rendered = tmpl.render(jinja_args)?;

    // Parse result into PromptAst
    Ok(parse_rendered_output(&rendered, ctx, &media_handles))
}

/// Preprocess template: dedent and trim.
///
/// Dedenting logic ported from engine/baml-lib/jinja-runtime/src/lib.rs:266-277
pub fn preprocess_template(template: &str) -> String {
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

fn add_globals(
    env: &mut Environment,
    ctx: &RenderContext,
    media_handles: &mut HashMap<usize, bex_vm_types::MediaValue>,
) -> Result<(), super::RenderPromptError> {
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
            tags => ctx.tags.iter().map(|(k, v)| Ok((k.clone(), external_value_to_jinja(v, media_handles)?))).collect::<Result<IndexMap<String, minijinja::value::Value>, super::RenderPromptError>>()?.into_iter().collect::<minijinja::value::Value>(),
            output_format => minijinja::value::Value::from_object(output_format),
            enums => enums_map,
        },
    );
    Ok(())
}

fn parse_rendered_output(
    rendered: &str,
    ctx: &RenderContext,
    media_handles: &HashMap<usize, bex_vm_types::MediaValue>,
) -> PromptAst {
    // Check if this is a chat-style prompt (contains role delimiters)
    if rendered.contains(MAGIC_CHAT_ROLE_DELIMITER) {
        parse_chat_prompt(rendered, ctx, media_handles)
    } else if rendered.contains(MAGIC_MEDIA_DELIMITER) {
        let content = parse_message_content(rendered, media_handles);
        PromptAst::Simple(std::sync::Arc::new(content))
    } else {
        rendered.to_string().into()
    }
}

fn parse_chat_prompt(
    rendered: &str,
    _ctx: &RenderContext,
    media_handles: &HashMap<usize, bex_vm_types::MediaValue>,
) -> PromptAst {
    let mut messages = Vec::new();

    // Split on role delimiter
    let parts: Vec<&str> = rendered.split(MAGIC_CHAT_ROLE_DELIMITER).collect();

    let mut current_role: Option<String> = None;
    let mut current_content = String::new();

    for part in parts {
        if part.starts_with(":baml-start-role:") && part.ends_with(":baml-end-role:") {
            // Save previous message if any
            if let Some(role) = current_role.take() {
                let content = parse_message_content(&current_content, media_handles);
                messages.push(PromptAst::Message {
                    role,
                    content: content.into(),
                    metadata: serde_json::Value::default(),
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
        let content = parse_message_content(&current_content, media_handles);
        messages.push(PromptAst::Message {
            role,
            content: std::sync::Arc::new(content),
            metadata: serde_json::Value::default(),
        });
    }

    if messages.is_empty() {
        rendered.to_string().into()
    } else if messages.len() == 1 {
        messages.pop().unwrap()
    } else {
        PromptAst::Vec(messages.into_iter().map(std::sync::Arc::new).collect())
    }
}

fn parse_message_content(
    content: &str,
    media_handles: &HashMap<usize, bex_vm_types::MediaValue>,
) -> PromptAstSimple {
    // Check for media delimiters
    if content.contains(MAGIC_MEDIA_DELIMITER) {
        let mut parts = Vec::new();
        let chunks: Vec<&str> = content.split(MAGIC_MEDIA_DELIMITER).collect();

        for (i, chunk) in chunks.iter().enumerate() {
            if i % 2 == 1 {
                // This is a media chunk - parse the handle
                // Format: :baml-start-media:{handle}:baml-end-media:
                if let Some(handle) = parse_media_handle(chunk) {
                    if let Some(media) = media_handles.get(&handle) {
                        parts.push(PromptAstSimple::Media(media.clone()));
                    } else {
                        // Handle not found (e.g. mismatched delimiter); treat as literal string
                        parts.push(PromptAstSimple::String("[media not found]".to_string()));
                    }
                }
            } else if !chunk.trim().is_empty() {
                parts.push(PromptAstSimple::String((*chunk).trim().to_string()));
            }
        }

        if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            PromptAstSimple::Multiple(parts.into_iter().map(std::sync::Arc::new).collect())
        }
    } else {
        PromptAstSimple::String(content.trim().to_string())
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
    use std::sync::Arc;

    use baml_type::Ty;

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
            output_format: OutputFormatContent::new(Ty::String {
                attr: baml_type::TyAttr::default(),
            }),
            tags: IndexMap::new(),
            enums: HashMap::new(),
        }
    }

    #[test]
    fn test_simple_string() {
        let template = "Hello, world!";
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, "Hello, world!".to_string().into());
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

        assert_eq!(result, "Hello, Alice!".to_string().into());
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

        assert_eq!(result, "Name: Bob, Age: 30".to_string().into());
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

        let expected = PromptAst::Vec(
            vec![
                PromptAst::Message {
                    role: "system".to_string(),
                    content: Arc::new(("You are a helpful assistant.".to_string()).into()),
                    metadata: serde_json::Value::default(),
                },
                PromptAst::Message {
                    role: "user".to_string(),
                    content: Arc::new("Hello!".to_string().into()),
                    metadata: serde_json::Value::default(),
                },
            ]
            .into_iter()
            .map(std::sync::Arc::new)
            .collect(),
        );

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
            content: Arc::new("Hello with default role!".to_string().into()),
            metadata: serde_json::Value::default(),
        };

        assert_eq!(result, expected);
    }

    #[test]
    fn test_dedent() {
        let template = r#"
            Hello,
            World!
        "#;
        let result = preprocess_template(template);
        assert_eq!(result, "Hello,\nWorld!");
    }

    #[test]
    fn test_array_iteration() {
        let template = "Items: {% for item in items %}{{ item }}{% if not loop.last %}, {% endif %}{% endfor %}";

        let mut args = IndexMap::new();
        args.insert(
            "items".to_string(),
            BexExternalValue::Array {
                element_type: Ty::String {
                    attr: baml_type::TyAttr::default(),
                },
                items: vec![
                    BexExternalValue::String("apple".to_string()),
                    BexExternalValue::String("banana".to_string()),
                    BexExternalValue::String("cherry".to_string()),
                ],
            },
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        assert_eq!(result, "Items: apple, banana, cherry".to_string().into());
    }

    #[test]
    fn test_output_format_in_template() {
        let template = "{{ ctx.output_format }}";
        let args = IndexMap::new();

        // Create a context with an int output format
        let mut ctx = test_ctx();
        ctx.output_format = OutputFormatContent::new(Ty::Int {
            attr: baml_type::TyAttr::default(),
        });

        let result = render_prompt(template, &args, &ctx).unwrap();

        assert_eq!(result, "Answer as an int".to_string().into());
    }

    #[test]
    fn test_output_format_with_kwargs() {
        let template = "{{ ctx.output_format(prefix='Please respond with: ') }}";
        let args = IndexMap::new();

        let mut ctx = test_ctx();
        ctx.output_format = OutputFormatContent::new(Ty::Int {
            attr: baml_type::TyAttr::default(),
        });

        let result = render_prompt(template, &args, &ctx).unwrap();

        assert_eq!(result, "Please respond with: int".to_string().into());
    }

    #[test]
    fn test_format_number_with_commas() {
        let template = r#"{{ "{:,}".format(1234567) }}"#;
        let args = IndexMap::new();
        let result = render_prompt(template, &args, &test_ctx()).unwrap();
        assert_eq!(result, "1,234,567".to_string().into());

        // float formatting
        let template = r#"{{ "{:.2f}".format(3.14159) }}"#;
        let result = render_prompt(template, &args, &test_ctx()).unwrap();
        assert_eq!(result, "3.14".to_string().into());

        // negative integers
        let template = r#"{{ "{:,}".format(-1234567) }}"#;
        let result = render_prompt(template, &args, &test_ctx()).unwrap();
        assert_eq!(result, "-1,234,567".to_string().into());
    }

    // ========================================================================
    // Media tests
    // ========================================================================

    fn make_media(
        kind: baml_base::MediaKind,
        content: baml_builtins2::MediaContent,
        mime: Option<&str>,
    ) -> std::sync::Arc<baml_builtins2::MediaValue> {
        std::sync::Arc::new(baml_builtins2::MediaValue::new(
            kind,
            content,
            mime.map(String::from),
        ))
    }

    fn make_media_external(
        kind: baml_base::MediaKind,
        content: baml_builtins2::MediaContent,
        mime: Option<&str>,
    ) -> BexExternalValue {
        BexExternalValue::Adt(bex_external_types::BexExternalAdt::Media(make_media(
            kind, content, mime,
        )))
    }

    fn make_media_instance(
        class_name: &str,
        kind: baml_base::MediaKind,
        content: baml_builtins2::MediaContent,
        mime: Option<&str>,
    ) -> BexExternalValue {
        let media = make_media_external(kind, content, mime);
        let mut fields = IndexMap::new();
        fields.insert("_data".to_string(), media);
        BexExternalValue::Instance {
            class_name: class_name.to_string(),
            fields,
        }
    }

    #[test]
    fn test_media_renders_as_prompt_ast_media() {
        let template = "Describe this: {{ img }}";
        let mut args = IndexMap::new();
        args.insert(
            "img".to_string(),
            make_media_external(
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/photo.jpg".into(),
                    base64_data: None,
                },
                Some("image/jpeg"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        // Should produce a Multiple with text + media
        match &result {
            PromptAst::Simple(content) => match content.as_ref() {
                PromptAstSimple::Multiple(parts) => {
                    assert_eq!(parts.len(), 2);
                    match parts[0].as_ref() {
                        PromptAstSimple::String(s) => assert_eq!(s, "Describe this:"),
                        other => panic!("Expected String, got {other:?}"),
                    }
                    match parts[1].as_ref() {
                        PromptAstSimple::Media(m) => {
                            assert_eq!(m.kind, baml_base::MediaKind::Image);
                        }
                        other => panic!("Expected Media, got {other:?}"),
                    }
                }
                other => panic!("Expected Multiple, got {other:?}"),
            },
            other => panic!("Expected Simple, got {other:?}"),
        }
    }

    #[test]
    fn test_media_in_chat_message() {
        let template = r#"
            {{ _.role("user") }}
            Look at this image:
            {{ img }}
        "#;

        let mut args = IndexMap::new();
        args.insert(
            "img".to_string(),
            make_media_external(
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Base64 {
                    base64_data: "iVBORw0KGgo=".into(),
                },
                Some("image/png"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        match &result {
            PromptAst::Message { role, content, .. } => {
                assert_eq!(role, "user");
                match content.as_ref() {
                    PromptAstSimple::Multiple(parts) => {
                        assert_eq!(parts.len(), 2);
                        assert!(matches!(parts[0].as_ref(), PromptAstSimple::String(_)));
                        assert!(matches!(parts[1].as_ref(), PromptAstSimple::Media(_)));
                    }
                    other => panic!("Expected Multiple, got {other:?}"),
                }
            }
            other => panic!("Expected Message, got {other:?}"),
        }
    }

    #[test]
    fn test_media_wrapper_instance_unwrapped() {
        let template = "{{ receipt }}";
        let mut args = IndexMap::new();
        args.insert(
            "receipt".to_string(),
            make_media_instance(
                "baml.media.Image",
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/receipt.jpg".into(),
                    base64_data: None,
                },
                Some("image/jpeg"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        // The wrapper instance should be unwrapped to produce a media node
        match &result {
            PromptAst::Simple(content) => match content.as_ref() {
                PromptAstSimple::Media(m) => {
                    assert_eq!(m.kind, baml_base::MediaKind::Image);
                }
                other => panic!("Expected Media, got {other:?}"),
            },
            other => panic!("Expected Simple, got {other:?}"),
        }
    }

    #[test]
    fn test_media_wrapper_audio() {
        let template = "{{ audio }}";
        let mut args = IndexMap::new();
        args.insert(
            "audio".to_string(),
            make_media_instance(
                "baml.media.Audio",
                baml_base::MediaKind::Audio,
                baml_builtins2::MediaContent::Base64 {
                    base64_data: "AAAA".into(),
                },
                Some("audio/wav"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        match &result {
            PromptAst::Simple(content) => match content.as_ref() {
                PromptAstSimple::Media(m) => {
                    assert_eq!(m.kind, baml_base::MediaKind::Audio);
                }
                other => panic!("Expected Media, got {other:?}"),
            },
            other => panic!("Expected Simple, got {other:?}"),
        }
    }

    #[test]
    fn test_media_wrapper_pdf() {
        let template = "{{ doc }}";
        let mut args = IndexMap::new();
        args.insert(
            "doc".to_string(),
            make_media_instance(
                "baml.media.Pdf",
                baml_base::MediaKind::Pdf,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/doc.pdf".into(),
                    base64_data: None,
                },
                Some("application/pdf"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        match &result {
            PromptAst::Simple(content) => match content.as_ref() {
                PromptAstSimple::Media(m) => {
                    assert_eq!(m.kind, baml_base::MediaKind::Pdf);
                }
                other => panic!("Expected Media, got {other:?}"),
            },
            other => panic!("Expected Simple, got {other:?}"),
        }
    }

    #[test]
    fn test_media_with_text_in_chat() {
        let template = r#"
            {{ _.role("user") }}
            Extract info from this receipt:
            {{ receipt }}
            {{ ctx.output_format }}
        "#;

        let mut args = IndexMap::new();
        args.insert(
            "receipt".to_string(),
            make_media_external(
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/receipt.jpg".into(),
                    base64_data: None,
                },
                Some("image/jpeg"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        match &result {
            PromptAst::Message { role, content, .. } => {
                assert_eq!(role, "user");
                match content.as_ref() {
                    PromptAstSimple::Multiple(parts) => {
                        // Should have: text, media (output_format for string is empty)
                        assert!(
                            parts.len() >= 2,
                            "Expected at least 2 parts, got {}",
                            parts.len()
                        );
                        assert!(matches!(parts[0].as_ref(), PromptAstSimple::String(_)));
                        assert!(matches!(parts[1].as_ref(), PromptAstSimple::Media(_)));
                    }
                    other => panic!("Expected Multiple, got {other:?}"),
                }
            }
            other => panic!("Expected Message, got {other:?}"),
        }
    }

    #[test]
    fn test_multiple_media_in_message() {
        let template = r#"
            {{ _.role("user") }}
            Compare these images:
            {{ img1 }}
            {{ img2 }}
        "#;

        let mut args = IndexMap::new();
        args.insert(
            "img1".to_string(),
            make_media_external(
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/a.jpg".into(),
                    base64_data: None,
                },
                Some("image/jpeg"),
            ),
        );
        args.insert(
            "img2".to_string(),
            make_media_external(
                baml_base::MediaKind::Image,
                baml_builtins2::MediaContent::Url {
                    url: "https://example.com/b.jpg".into(),
                    base64_data: None,
                },
                Some("image/png"),
            ),
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        match &result {
            PromptAst::Message { content, .. } => match content.as_ref() {
                PromptAstSimple::Multiple(parts) => {
                    let media_count = parts
                        .iter()
                        .filter(|p| matches!(p.as_ref(), PromptAstSimple::Media(_)))
                        .count();
                    assert_eq!(media_count, 2, "Should have 2 media parts");
                }
                other => panic!("Expected Multiple, got {other:?}"),
            },
            other => panic!("Expected Message, got {other:?}"),
        }
    }

    #[test]
    fn test_non_media_instance_not_unwrapped() {
        let template = "{{ person }}";
        let mut fields = IndexMap::new();
        fields.insert(
            "name".to_string(),
            BexExternalValue::String("Alice".to_string()),
        );
        let mut args = IndexMap::new();
        args.insert(
            "person".to_string(),
            BexExternalValue::Instance {
                class_name: "user.Person".to_string(),
                fields,
            },
        );

        let result = render_prompt(template, &args, &test_ctx()).unwrap();

        // Non-media instances should render as maps, not be unwrapped
        match &result {
            PromptAst::Simple(content) => match content.as_ref() {
                PromptAstSimple::String(s) => {
                    assert!(s.contains("Alice"), "Should contain field value: {s}");
                }
                other => panic!("Expected String, got {other:?}"),
            },
            other => panic!("Expected Simple, got {other:?}"),
        }
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

        assert_eq!(result, "Category: SPORTS".to_string().into());
    }
}
