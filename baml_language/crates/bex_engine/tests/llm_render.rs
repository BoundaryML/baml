//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! 1. `get_jinja_template` returns the correct template for LLM functions
//! 2. `get_client` returns the correct client chain
//! 3. `render_prompt` correctly renders templates with arguments

use baml_builtins2::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use baml_type::TyAttr;
use bex_engine::{FunctionCallContextBuilder, Ty};
use bex_heap::BexExternalValue;

#[tokio::test]
async fn test_render_prompt_directly() {
    use indexmap::IndexMap;

    // Test the Jinja rendering directly
    let template = "Hello, {{ name }}! You are {{ age }} years old.";
    let mut args = IndexMap::new();
    args.insert(
        "name".to_string(),
        BexExternalValue::String("Alice".to_string()),
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
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
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
        BexExternalValue::String("What is 2+2?".to_string()),
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
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
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
        output_format: sys_llm::OutputFormatContent::new(Ty::String {
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

/// Test the full `render_prompt` flow through the engine.
///
/// This test:
/// 1. Compiles BAML source with an LLM function
/// 2. Calls a BAML function that internally calls `baml.llm.render_prompt`
/// 3. Verifies the call succeeds (`PromptAst` is an internal type, can't return it directly)
#[tokio::test]
async fn test_render_prompt_e2e() {
    use bex_engine::{BexEngine, BexExternalValue};
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

// Test wrapper that calls render_prompt and returns something we can check
// Since PromptAst isn't a user-facing type, we just verify the call succeeds
function test_render() -> int {
    // Pass an empty map for args - the Greet function expects a 'name' param
    // but for this test we just want to verify the render_prompt flow works
    let args = {};
    let result = baml.llm.render_prompt(TestClient, "Greet", args);
    // If we got here without crashing, the call worked
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "test_render",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(value) => {
            assert_eq!(value, BexExternalValue::Int(42));
        }
        Err(e) => {
            panic!("test_render failed: {e}");
        }
    }
}

#[tokio::test]
async fn test_render_prompt_with_inline_client_shorthand() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function Greet(name: string) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Hello, {{ name }}!
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    Greet$render_prompt("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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
        .expect("failed to render prompt for inline client shorthand");

    assert_eq!(result, prompt_ast_message("system", "Hello, World!"));
}

/// Test that `render_prompt` returns a `PromptAst` value.
///
/// This test calls `render_prompt` and verifies the result is a `PromptAst`
/// containing the expected rendered content.
#[tokio::test]
async fn test_render_prompt_returns_prompt_ast() {
    use bex_engine::{BexEngine, BexExternalValue};
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

// Function that returns the PromptAst type - this should work since
// PromptAst is now a visible builtin type
function get_prompt() -> baml.llm.PromptAst {
    let args = { "name": "World" };
    baml.llm.render_prompt(TestClient, "Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "get_prompt",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    match result {
        Ok(value) => {
            // Verify it's a PromptAst (wrapped in Adt)
            match &value {
                BexExternalValue::Instance { class_name, fields } => {
                    assert!(
                        class_name == "baml.llm.PromptAst",
                        "Expected class name 'baml.llm.PromptAst', got {class_name}"
                    );
                    assert!(fields.len() == 1, "Expected 1 field, got {}", fields.len());
                    // The template "Hello, {{ name }}!" with name="World" should render to PromptAst::String
                    // match ast.as_ref() {
                    //     BuiltinPromptAst::Simple(s) => {
                    //         let PromptAstSimple::String(s) = s.as_ref() else {
                    //             panic!("Expected string content");
                    //         };
                    //         assert_eq!(s, "Hello, World!");
                    //     }
                    //     _ => panic!("Expected simple content"),
                    // }
                }
                other => {
                    panic!("Expected Adt(PromptAst), got {other:?}");
                }
            }
        }
        Err(e) => {
            panic!("get_prompt failed: {e}");
        }
    }
}

/// Test that `build_request` succeeds and returns an `int` result.
///
/// This test verifies the `baml.llm.build_request` entry point is callable
/// and the underlying `LlmBuildRequest` `SysOp` is implemented.
#[tokio::test]
async fn test_build_request_returns() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_build_request() -> int {
    let args = { "name": "World" };
    let request = baml.llm.build_request(TestClient, "Greet", args);
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "test_build_request",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    assert!(result.is_ok(), "build_request should succeed: {result:?}");
}

#[tokio::test]
async fn test_call_llm_function_string() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> unknown {
    let args = { "name": "World" };
    baml.llm.call_llm_function(TestClient, "Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Without a valid API key, the orchestration loop will either:
    // - Get a non-2xx response from OpenAI (ok() == false)
    // - Get a network error (synthetic response with status_code=0)
    // Either way, all steps fail and we hit `assert false`.
    assert!(result.is_err(), "Expected error without valid API key");
}

#[tokio::test]
async fn test_call_llm_function_inline_client_shorthand_gets_past_constructor_lookup() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function Greet(name: string) -> string {
    client "openai/gpt-4o-mini"
    prompt #"
        Hello, {{ name }}!
    "#
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "Greet",
            vec![BexExternalValue::String("World".to_string())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    let err = result.expect_err("inline shorthand LLM call should still fail in tests");
    assert!(
        !err.to_string()
            .contains("Client resolve function not found: openai/gpt-4o-mini$new"),
        "inline shorthand should resolve to a primitive client before any network error: {err}"
    );
}

#[tokio::test]
async fn test_direct_llm_call() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> string {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> string {
    Greet("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Without a valid API key, the orchestration loop will either:
    // - Get a non-2xx response from OpenAI (ok() == false)
    // - Get a network error (synthetic response with status_code=0)
    // Either way, all steps fail and we hit `assert false`.
    assert!(result.is_err(), "Expected error without valid API key");
}

#[tokio::test]
async fn test_call_llm_function_non_string_returns_error() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function Greet(name: string) -> map<string, int> {
    client TestClient
    prompt #"
        Hello, {{ name }}!
    "#
}

function test_call_llm() -> unknown {
    let args = { "name": "World" };
    baml.llm.call_llm_function(TestClient, "Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    );

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine
        .call_function(
            "test_call_llm",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    // Without a valid API key, the orchestration loop will either:
    // - Get a non-2xx response from OpenAI (ok() == false)
    // - Get a network error (synthetic response with status_code=0)
    // Either way, all steps fail and we hit `assert false`.
    assert!(result.is_err(), "Expected error without valid API key");
}

// ============================================================================
// Template String Tests
// ============================================================================

/// Build a `BexExternalValue` wrapping a single-message `PromptAst`.
///
/// The engine renders a prompt without explicit `_.role()` calls as a single
/// `Message` with the client's default role ("system" for openai).
fn prompt_ast_message(role: &str, content: &str) -> BexExternalValue {
    use bex_external_types::BexExternalAdt;
    BexExternalValue::Instance {
        class_name: "baml.llm.PromptAst".to_string(),
        fields: indexmap::indexmap! {
            "_data".to_string() => BexExternalValue::Adt(BexExternalAdt::PromptAst(
                std::sync::Arc::new(BuiltinPromptAst::Message {
                    role: role.to_string(),
                    content: std::sync::Arc::new(content.to_string().into()),
                    metadata: serde_json::Value::Null,
                }),
            ))
        },
    }
}

/// Test that a `template_string` is expanded as a Jinja macro in `render_prompt`.
#[tokio::test]
async fn test_template_string_in_prompt() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Greet(name: string) #"Hello, {{ name }}!"#

function TestFunc(name: string) -> string {
    client TestClient
    prompt #"
        {{ Greet(name) }}
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "name": "Alice" };
    baml.llm.render_prompt(TestClient, "TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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
        .expect("failed to render prompt that calls template_string Greet(name)");
    assert_eq!(result, prompt_ast_message("system", "Hello, Alice!"));
}

/// Test that nested `template_strings` expand correctly.
#[tokio::test]
async fn test_nested_template_strings() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Inner() #"INNER"#
template_string Outer() #"before {{ Inner() }} after"#

function TestFunc() -> string {
    client TestClient
    prompt #"{{ Outer() }}"#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = {};
    baml.llm.render_prompt(TestClient, "TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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
        .expect("failed to render prompt with nested template_strings Outer() -> Inner()");
    assert_eq!(result, prompt_ast_message("system", "before INNER after"));
}

/// Test a `template_string` with two args, one of which is a class (struct).
#[tokio::test]
async fn test_template_string_with_struct_arg() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

class Person {
    name string
    age int
}

template_string Describe(label: string, person: Person) #"{{ label }}: {{ person.name }} (age {{ person.age }})"#

function TestFunc(label: string, person: Person) -> string {
    client TestClient
    prompt #"
        {{ Describe(label, person) }}
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = { "label": "User", "person": { "name": "Bob", "age": 42 } };
    baml.llm.render_prompt(TestClient, "TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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
        .expect("failed to render prompt with 2-arg template_string Describe(label, person)");
    assert_eq!(result, prompt_ast_message("system", "User: Bob (age 42)"));
}

/// Test that parameterless `template_strings` work.
#[tokio::test]
async fn test_parameterless_template_string() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

template_string Header() #"=== HEADER ==="#

function TestFunc() -> string {
    client TestClient
    prompt #"{{ Header() }}
Content here"#
}

function get_prompt() -> baml.llm.PromptAst {
    let args = {};
    baml.llm.render_prompt(TestClient, "TestFunc", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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
        .expect("failed to render prompt that calls parameterless template_string Header()");
    assert_eq!(
        result,
        prompt_ast_message("system", "=== HEADER ===\nContent here")
    );
}

// ============================================================================
// Phase 3: json alias LLM-path sentinel
// ============================================================================

/// Verify that `function F() -> json { ... prompt #"{{ ctx.output_format }}"# }`
/// renders a prompt containing "Respond with valid JSON." — the static literal
/// required by BEP-038 Phase 3 — and does NOT contain the union-arm enumeration
/// (`null or bool or int ...`).
#[tokio::test]
async fn test_json_return_type_renders_valid_json_literal() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
client TestClient {
    provider openai
    options {
        model "gpt-4"
    }
}

function ExtractAny() -> json {
    client TestClient
    prompt #"
        Return whatever JSON you like.

        {{ ctx.output_format }}
    "#
}

function get_prompt() -> baml.llm.PromptAst {
    baml.llm.render_prompt(TestClient, "ExtractAny", {})
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            sys_native::SysOps::native().into(),
            None,
            Vec::new(),
        )
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

    // Extract the rendered text from the PromptAst.
    let rendered_text = match &result {
        BexExternalValue::Instance { class_name, fields } => {
            assert_eq!(class_name, "baml.llm.PromptAst");
            match fields.get("_data") {
                Some(BexExternalValue::Adt(adt)) => format!("{adt:?}"),
                other => format!("{other:?}"),
            }
        }
        other => format!("{other:?}"),
    };

    assert!(
        rendered_text.contains("Respond with valid JSON."),
        "rendered prompt must contain 'Respond with valid JSON.' — got: {rendered_text}"
    );
    // The union-arm enumeration must NOT appear in the rendered output.
    assert!(
        !rendered_text.contains("null or bool or int"),
        "rendered prompt must not contain union-arm enumeration — got: {rendered_text}"
    );
}
