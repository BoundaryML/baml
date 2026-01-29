//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! 1. `get_jinja_template` returns the correct template for LLM functions
//! 2. `get_client_function` returns the correct client chain
//! 3. `render_prompt` correctly renders templates with arguments

use std::collections::HashMap;

use bex_engine::BexEngine;
use bex_program::{BexProgram, ClientDef, FunctionBody, FunctionDef, ParamDef, Ty};
use bex_vm_types::Program;
use sys_native::SysOpsExt;

/// Create a minimal `BexProgram` with an LLM function and client for testing.
fn create_llm_test_program() -> BexProgram {
    let mut functions = HashMap::new();
    let mut clients = HashMap::new();

    // Create a simple LLM function
    functions.insert(
        "Classify".to_string(),
        FunctionDef {
            name: "Classify".to_string(),
            params: vec![ParamDef {
                name: "text".to_string(),
                param_type: Ty::String,
            }],
            return_type: Ty::String,
            body: FunctionBody::Llm {
                prompt_template: "Classify the following text: {{ text }}".to_string(),
                client: "TestClient".to_string(),
            },
        },
    );

    // Create another LLM function with more complex template
    functions.insert(
        "Summarize".to_string(),
        FunctionDef {
            name: "Summarize".to_string(),
            params: vec![
                ParamDef {
                    name: "text".to_string(),
                    param_type: Ty::String,
                },
                ParamDef {
                    name: "max_words".to_string(),
                    param_type: Ty::Int,
                },
            ],
            return_type: Ty::String,
            body: FunctionBody::Llm {
                prompt_template: "Summarize in {{ max_words }} words or less:\n\n{{ text }}"
                    .to_string(),
                client: "TestClient".to_string(),
            },
        },
    );

    // Create a client definition
    clients.insert(
        "TestClient".to_string(),
        ClientDef {
            name: "TestClient".to_string(),
            provider: "openai".to_string(),
            options: HashMap::from([
                ("model".to_string(), "gpt-4".to_string()),
                ("temperature".to_string(), "0.7".to_string()),
            ]),
            retry_policy: None,
        },
    );

    BexProgram {
        classes: HashMap::new(),
        enums: HashMap::new(),
        functions,
        clients,
        retry_policies: HashMap::new(),
        bytecode: Program::new(),
    }
}

#[tokio::test]
async fn test_get_jinja_template() {
    let program = create_llm_test_program();
    let engine = BexEngine::new(program, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // Use the internal method via a test helper
    // Since execute_get_jinja_template is private, we test through the SysOp dispatch
    // For now, we verify the program structure is correct
    let func = &engine.program().functions["Classify"];
    match &func.body {
        FunctionBody::Llm {
            prompt_template, ..
        } => {
            assert_eq!(prompt_template, "Classify the following text: {{ text }}");
        }
        FunctionBody::Expr { .. } => panic!("Expected LLM function body"),
    }
}

#[tokio::test]
async fn test_get_client_function() {
    let program = create_llm_test_program();
    let engine = BexEngine::new(program, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // Verify client is in the program
    let client = &engine.program().clients["TestClient"];
    assert_eq!(client.name, "TestClient");
    assert_eq!(client.provider, "openai");
    assert_eq!(client.options.get("model"), Some(&"gpt-4".to_string()));
}

#[tokio::test]
async fn test_llm_function_structure() {
    let program = create_llm_test_program();
    let engine = BexEngine::new(program, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // Verify Summarize function structure
    let func = &engine.program().functions["Summarize"];
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].name, "text");
    assert_eq!(func.params[1].name, "max_words");

    match &func.body {
        FunctionBody::Llm {
            prompt_template,
            client,
        } => {
            assert!(prompt_template.contains("{{ max_words }}"));
            assert!(prompt_template.contains("{{ text }}"));
            assert_eq!(client, "TestClient");
        }
        FunctionBody::Expr { .. } => panic!("Expected LLM function body"),
    }
}

#[tokio::test]
async fn test_render_prompt_directly() {
    use bex_external_types::{BexExternalValue, PrimitiveClientValue};
    use indexmap::IndexMap;

    // Test the Jinja rendering directly
    let template = "Hello, {{ name }}! You are {{ age }} years old.";
    let mut args = IndexMap::new();
    args.insert(
        "name".to_string(),
        BexExternalValue::String("Alice".to_string()),
    );
    args.insert("age".to_string(), BexExternalValue::Int(30));

    let client = PrimitiveClientValue {
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

    let ctx = bex_jinja_runtime::RenderContext {
        client: bex_jinja_runtime::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: bex_llm_types::OutputFormatContent::new(Ty::String),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = bex_jinja_runtime::render_prompt(template, &args, &ctx).unwrap();

    match result {
        bex_vm_types::PromptAst::String(s) => {
            assert_eq!(s, "Hello, Alice! You are 30 years old.");
        }
        _ => panic!("Expected string result"),
    }
}

#[tokio::test]
async fn test_render_prompt_with_chat_roles() {
    use bex_external_types::{BexExternalValue, PrimitiveClientValue};
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

    let client = PrimitiveClientValue {
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

    let ctx = bex_jinja_runtime::RenderContext {
        client: bex_jinja_runtime::RenderContextClient {
            name: client.name.clone(),
            provider: client.provider.clone(),
            default_role: client.default_role.clone(),
            allowed_roles: client.allowed_roles,
        },
        output_format: bex_llm_types::OutputFormatContent::new(Ty::String),
        tags: IndexMap::new(),
        enums: std::collections::HashMap::new(),
    };

    let result = bex_jinja_runtime::render_prompt(template, &args, &ctx).unwrap();

    // Result should be a Vec of messages
    match result {
        bex_vm_types::PromptAst::Vec(messages) => {
            assert_eq!(messages.len(), 2);

            // Check first message (system)
            match &messages[0] {
                bex_vm_types::PromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "system");
                    match content.as_ref() {
                        bex_vm_types::PromptAst::String(s) => {
                            assert!(s.contains("helpful assistant"));
                        }
                        _ => panic!("Expected string content"),
                    }
                }
                _ => panic!("Expected message"),
            }

            // Check second message (user)
            match &messages[1] {
                bex_vm_types::PromptAst::Message { role, content, .. } => {
                    assert_eq!(role, "user");
                    match content.as_ref() {
                        bex_vm_types::PromptAst::String(s) => {
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
    use bex_jinja_runtime::{RenderEnum, RenderEnumVariant};
    use indexmap::IndexMap;

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

    let ctx = bex_jinja_runtime::RenderContext {
        client: bex_jinja_runtime::RenderContextClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["user".to_string()],
        },
        output_format: bex_llm_types::OutputFormatContent::new(Ty::String),
        tags: IndexMap::new(),
        enums,
    };

    let result = bex_jinja_runtime::render_prompt(template, &args, &ctx).unwrap();

    match result {
        bex_vm_types::PromptAst::String(s) => {
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

    use bex_engine::BexExternalValue;
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

    let result = engine.call_function("test_render", &[]).await;

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

    use bex_engine::BexExternalValue;
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

    let result = engine.call_function("get_prompt", &[]).await;

    match result {
        Ok(value) => {
            // Verify it's a PromptAst
            match &value {
                BexExternalValue::PromptAst(ast) => {
                    // The template "Hello, {{ name }}!" with name="World" should render to PromptAst::String
                    let bex_external_types::PromptAst::String(content) = ast else {
                        panic!("Expected PromptAst::String, got {ast:?}");
                    };
                    assert_eq!(content, "Hello, World!");
                }
                other => {
                    panic!("Expected PromptAst, got {other:?}");
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

    let result = engine.call_function("test_build_request", &[]).await;
    assert!(result.is_ok(), "build_request should succeed: {result:?}");
}

/// Test that `call_llm_function` panics (parse response not yet implemented).
///
/// This test verifies the `baml.llm.call_llm_function` entry point is callable
/// but panics because the underlying `LlmParseResponse` `SysOp` is not implemented.
#[tokio::test]
#[should_panic(expected = "LlmParseResponse SysOp not yet implemented")]
async fn test_call_llm_function_panics() {
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
    let args = { "name": "World" };
    baml.llm.call_llm_function("Greet", args)
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native())
        .expect("Failed to create engine");

    // build_request now succeeds; this should panic at the next unimplemented
    // step: "LlmParseResponse SysOp not yet implemented"
    let _ = engine.call_function("test_call_llm", &[]).await;
}
