//! Integration tests for LLM prompt rendering.
//!
//! Rust-level tests drive `sys_llm` template rendering directly; BAML-level
//! tests drive the compiler-generated `<Fn>$render_prompt` companion, which
//! renders the spec's prompt (with the return type's output format) as a
//! structural `ai.Prompt`.
//!
//! Removed with the legacy LLM path (see git history): the
//! `baml.prompt.render_prompt`/`build_request`/`call_llm_function` builtin flow
//! over declared `client<llm>` blocks and Jinja prompts, and the
//! `template_string`-in-prompt expansion tests (`template_string` calls do not
//! bind inside ai-world backtick prompts).

use baml_builtins2::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use baml_type::TyAttr;
use bex_engine::{FunctionCallContextBuilder, RuntimeTy};
use bex_heap::BexExternalValue;

#[tokio::test]
async fn test_render_prompt_directly() {
    use indexmap::IndexMap;

    // Test the Jinja rendering directly
    let template = "Hello, {{ name }}! You are {{ age }} years old.";
    let mut args = IndexMap::new();
    args.insert(
        "name".to_string(),
        BexExternalValue::String("Alice".to_string().into()),
    );
    args.insert("age".to_string(), BexExternalValue::Int(30));

    let client =
        sys_llm::baml_std::PrimitiveClient::new("test".to_string(), "openai".to_string(), {
            sys_llm::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                default_role: Some("user".to_string()),
                allowed_roles: Some(vec![
                    "user".to_string(),
                    "assistant".to_string(),
                    "system".to_string(),
                ]),
                ..Default::default()
            }
        })
        .unwrap();

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(RuntimeTy::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    match result {
        BuiltinPromptAst::Simple(s) => {
            assert_eq!(
                s,
                std::sync::Arc::new("Hello, Alice! You are 30 years old.".to_string().into())
            );
        }
        _ => panic!("Expected string result"),
    }
}

#[tokio::test]
async fn test_render_prompt_with_chat_roles() {
    use indexmap::IndexMap;

    let template = r#"
{{ _.role("system") }}
You are a helpful assistant.
{{ _.role("user") }}
{{ question }}
"#;
    let mut args = IndexMap::new();
    args.insert(
        "question".to_string(),
        BexExternalValue::String("What is 2+2?".to_string().into()),
    );

    let client =
        sys_llm::baml_std::PrimitiveClient::new("test".to_string(), "openai".to_string(), {
            sys_llm::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                default_role: Some("user".to_string()),
                allowed_roles: Some(vec![
                    "user".to_string(),
                    "assistant".to_string(),
                    "system".to_string(),
                ]),
                ..Default::default()
            }
        })
        .unwrap();

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(RuntimeTy::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    // Result should be a Vec of messages
    match result {
        BuiltinPromptAst::Vec(messages) => {
            assert_eq!(messages.len(), 2);

            // Check first message (system)
            match messages[0].as_ref() {
                BuiltinPromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "system");
                    match content.as_ref() {
                        PromptAstSimple::String(s) => {
                            assert!(s.contains("helpful assistant"));
                        }
                        _ => panic!("Expected string content"),
                    }
                }
                _ => panic!("Expected message"),
            }

            // Check second message (user)
            match messages[1].as_ref() {
                BuiltinPromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "user");
                    match content.as_ref() {
                        PromptAstSimple::String(s) => {
                            assert!(s.contains("What is 2+2?"));
                        }
                        _ => panic!("Expected string content"),
                    }
                }
                _ => panic!("Expected message"),
            }
        }
        _ => panic!("Expected Vec of messages, got {result:?}"),
    }
}

#[tokio::test]
async fn test_render_prompt_with_enums() {
    use indexmap::IndexMap;
    use sys_llm::{RenderEnum, RenderEnumVariant};

    let template = "Category: {{ ctx.enums.Category.SPORTS }}";
    let args = IndexMap::new();

    let mut enums = std::collections::HashMap::new();
    enums.insert(
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

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string()],
        },
        output_format: sys_llm::OutputFormatContent::new(RuntimeTy::String {
            attr: TyAttr::default(),
        }),
        tags: IndexMap::new(),
        enums,
    };

    let result = sys_llm::render_prompt(template, &args, &ctx).unwrap();

    match result {
        BuiltinPromptAst::Simple(s) => {
            let PromptAstSimple::String(s) = s.as_ref() else {
                panic!("Expected string content");
            };
            assert_eq!(s, "Category: SPORTS");
        }
        _ => panic!("Expected string result"),
    }
}

mod common;

#[tokio::test]
async fn test_render_prompt_e2e_includes_output_format_schema() {
    let rendered = common::render_output_format(
        r#"
class Sentiment {
    feeling string @description("The detected sentiment")
    confidence float @description("Confidence score between 0 and 1")
    reasoning string @description("Brief explanation")
}
"#,
        "Sentiment",
    )
    .await;

    assert!(
        rendered.contains("Answer in JSON using this schema:"),
        "prompt did not include output format prefix:\n{rendered}"
    );
    assert!(
        rendered.contains("feeling")
            && rendered.contains("confidence")
            && rendered.contains("reasoning"),
        "prompt did not include return class fields:\n{rendered}"
    );
}

#[tokio::test]
async fn test_output_format_can_render_null_as_omit() {
    let rendered = common::render_output_format_with_opts(
        r#"
class Person {
    name string
    nickname string?
}
"#,
        "Person",
        "render_null_as = \"omit\"",
    )
    .await;

    assert!(
        rendered.contains("nickname: string or omit,"),
        "nullable field should use the custom null label:\n{rendered}"
    );
    assert!(
        !rendered.contains("string or null"),
        "custom null label should replace default null rendering:\n{rendered}"
    );
}

/// The `$render_prompt` companion renders the prompt offline as an `ai.Prompt`.
/// Provider construction is pure in the ai world (credentials
/// resolve from the environment at request time), so rendering never needs
/// an `api_key` env var — the B-626 guarantee, now structural.
#[tokio::test]
async fn test_render_prompt_offline_without_api_key_env() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
class C { x: string }

client Fast = openai.OpenAiClient.new(model = "gpt-4o-mini");

function Extract(raw: string) -> C {
    client Fast
    prompt `
        Extract from ${raw}.
        ${ctx.output_format}
    `
}

function get_prompt() -> string {
    Extract$render_prompt("hello").text()
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("render_prompt must succeed offline without any api_key env var");

    let BexExternalValue::String(rendered) = result else {
        panic!("expected $render_prompt to return a string, got {result:?}");
    };
    assert!(
        rendered.contains("Extract from hello."),
        "rendered prompt should contain the interpolated arg, got: {rendered}"
    );
    assert!(
        rendered.contains("x: string"),
        "rendered prompt should contain the output-format schema, got: {rendered}"
    );
}

/// An inline `"provider/model"` shorthand client renders through
/// `$render_prompt` exactly like a declared client value.
#[tokio::test]
async fn test_render_prompt_with_inline_client_shorthand() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function Greet(name: string) -> string {
    client "openai/gpt-4o-mini"
    prompt `Hello, ${name}!`
}

function get_prompt() -> string {
    Greet$render_prompt("World").text()
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("shorthand-client $render_prompt should succeed offline");

    let BexExternalValue::String(rendered) = result else {
        panic!("expected $render_prompt to return a string, got {result:?}");
    };
    assert!(
        rendered.contains("Hello, World!"),
        "rendered prompt should contain the interpolated arg, got: {rendered}"
    );
}

// ============================================================================
// Phase 3: json alias LLM-path sentinel
// ============================================================================

/// Verify that a `-> json` LLM function renders a prompt containing
/// "Respond with valid JSON." — the static literal required by BEP-038
/// Phase 3 — and does NOT contain the union-arm enumeration
/// (`null or bool or int ...`).
#[tokio::test]
async fn test_json_return_type_renders_valid_json_literal() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function ExtractAny() -> json {
    client "openai/gpt-4o"
    prompt `
        Return whatever JSON you like.

        ${ctx.output_format}
    `
}

function get_prompt() -> string {
    ExtractAny$render_prompt().text()
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("failed to render prompt for json return type");

    let BexExternalValue::String(rendered) = result else {
        panic!("expected $render_prompt to return a string, got {result:?}");
    };
    assert!(
        rendered.contains("Respond with valid JSON."),
        "rendered prompt must contain 'Respond with valid JSON.' — got: {rendered}"
    );
    assert!(
        !rendered.contains("null or bool or int"),
        "rendered prompt must not contain union-arm enumeration — got: {rendered}"
    );
}
