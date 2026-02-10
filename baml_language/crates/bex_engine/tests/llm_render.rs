//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! 1. `get_jinja_template` returns the correct template for LLM functions
//! 2. `get_client_function` returns the correct client chain
//! 3. `render_prompt` correctly renders templates with arguments

use baml_builtins::{PromptAst as BuiltinPromptAst, PromptAstSimple};
use bex_engine::{EngineError, Ty};
use bex_external_types::BexExternalAdt;
use bex_heap::{BexExternalValue, builtin_types::owned::LlmPrimitiveClient};
use sys_types::{OpError, OpErrorKind, SysOp};

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

    let client = LlmPrimitiveClient {
        name: "test".to_string(),
        provider: "openai".to_string(),
        default_role: "user".to_string(),
        allowed_roles: vec![
            "user".to_string(),
            "assistant".to_string(),
            "system".to_string(),
        ],
        options: IndexMap::new(),
    };

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(Ty::String),
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

    let client = LlmPrimitiveClient {
        name: "test".to_string(),
        provider: "openai".to_string(),
        default_role: "user".to_string(),
        allowed_roles: vec![
            "user".to_string(),
            "assistant".to_string(),
            "system".to_string(),
        ],
        options: IndexMap::new(),
    };

    let ctx = sys_llm::RenderContext {
        client: sys_llm::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: sys_llm::OutputFormatContent::new(Ty::String),
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
        output_format: sys_llm::OutputFormatContent::new(Ty::String),
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

/// Test the full `render_prompt` flow through the engine.
///
/// This test:
/// 1. Compiles BAML source with an LLM function
/// 2. Calls a BAML function that internally calls `baml.llm.render_prompt`
/// 3. Verifies the call succeeds (`PromptAst` is an internal type, can't return it directly)
#[tokio::test]
async fn test_render_prompt_e2e() {
    use std::collections::HashMap;

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
    let result = baml.llm.render_prompt("Greet", args);
    // If we got here without crashing, the call worked
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    let result = engine.call_function("test_render", vec![]).await;

    match result {
        Ok(value) => {
            assert_eq!(value, BexExternalValue::Int(42));
        }
        Err(e) => {
            panic!("test_render failed: {e}");
        }
    }
}

/// Test that `render_prompt` returns a `PromptAst` value.
///
/// This test calls `render_prompt` and verifies the result is a `PromptAst`
/// containing the expected rendered content.
#[tokio::test]
async fn test_render_prompt_returns_prompt_ast() {
    use std::collections::HashMap;

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
function get_prompt() -> PromptAst {
    let args = { "name": "World" };
    baml.llm.render_prompt("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    let result = engine.call_function("get_prompt", vec![]).await;

    match result {
        Ok(value) => {
            // Verify it's a PromptAst (wrapped in Adt)
            match &value {
                BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => {
                    // The template "Hello, {{ name }}!" with name="World" should render to PromptAst::String
                    match ast.as_ref() {
                        BuiltinPromptAst::Simple(s) => {
                            let PromptAstSimple::String(s) = s.as_ref() else {
                                panic!("Expected string content");
                            };
                            assert_eq!(s, "Hello, World!");
                        }
                        _ => panic!("Expected simple content"),
                    }
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
    use std::collections::HashMap;

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
    let request = baml.llm.build_request("Greet", args);
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    let result = engine.call_function("test_build_request", vec![]).await;
    assert!(result.is_ok(), "build_request should succeed: {result:?}");
}

#[tokio::test]
async fn test_call_llm_function_string() {
    use std::collections::HashMap;

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
    baml.llm.call_llm_function("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine.call_function("test_call_llm", vec![]).await;

    match result {
        Ok(value) => {
            // Verify we got an error response without asserting exact upstream message
            if let BexExternalValue::String(s) = &value {
                assert!(s.contains("error"), "Expected error response, got: {s}");
                assert!(
                    s.contains("invalid_request_error") || s.contains("API key"),
                    "Expected API key error, got: {s}"
                );
            } else {
                panic!("Expected String result, got {value:?}");
            }
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlHttpSend,
            kind: OpErrorKind::Other(message),
        })) if message.contains("HTTP request failed for") => {
            // network failed
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlLlmPrimitiveClientParse,
            kind: OpErrorKind::LlmClientError { message },
        })) if message.contains("You didn't provide an API key.") => {
            // this is ok, we had an API Error due to invalid API keys
        }
        Err(e) => {
            panic!("test_call_llm failed: {e:?}");
        }
    }
}

#[tokio::test]
async fn test_direct_llm_call() {
    use std::collections::HashMap;

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
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine.call_function("test_call_llm", vec![]).await;

    match result {
        Ok(value) => {
            // Verify we got an error response without asserting exact upstream message
            if let BexExternalValue::String(s) = &value {
                assert!(s.contains("error"), "Expected error response, got: {s}");
                assert!(
                    s.contains("invalid_request_error") || s.contains("API key"),
                    "Expected API key error, got: {s}"
                );
            } else {
                panic!("Expected String result, got {value:?}");
            }
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlHttpSend,
            kind: OpErrorKind::Other(message),
        })) if message.contains("HTTP request failed for") => {
            // network failed
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlLlmPrimitiveClientParse,
            kind: OpErrorKind::LlmClientError { message },
        })) if message.contains("You didn't provide an API key.") => {
            // this is ok, we had an API Error due to invalid API keys
        }
        Err(e) => {
            panic!("test_direct_llm_call failed: {e:?}");
        }
    }
}

#[tokio::test]
async fn test_call_llm_function_non_string_returns_error() {
    use std::collections::HashMap;

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
    baml.llm.call_llm_function("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let result = engine.call_function("test_call_llm", vec![]).await;

    match result {
        Ok(value) => {
            panic!("test_call_llm should return an error: {value:?}");
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlHttpSend,
            kind: OpErrorKind::Other(message),
        })) if message.contains("HTTP request failed for") => {
            // network failed
        }
        Err(EngineError::ExternalOpFailed(OpError {
            fn_name: SysOp::BamlLlmPrimitiveClientParse,
            kind: OpErrorKind::LlmClientError { message },
        })) if message.contains("You didn't provide an API key.") => {
            // this is ok, we had an API Error due to invalid API keys
        }
        Err(e) => {
            assert!(
                matches!(
                    e,
                    bex_engine::EngineError::ExternalOpFailed(sys_types::OpError {
                        kind: sys_types::OpErrorKind::NotImplemented { message: _ },
                        fn_name: SysOp::BamlLlmPrimitiveClientParse,
                    })
                ),
                "Expected NotImplemented error, got {e}"
            );
        }
    }
}
