//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! These tests exercise compiled backtick prompt companions and request building.

use baml_builtins2::PromptAst as BuiltinPromptAst;
use bex_engine::FunctionCallContextBuilder;
use bex_heap::BexExternalValue;

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
    prompt `
        Hello, ${name}!
    `
}

// Test wrapper that calls render_prompt and returns something we can check
// Since PromptAst isn't a user-facing type, we just verify the call succeeds
function test_render() -> int {
    // Pass an empty map for args - the Greet function expects a 'name' param
    // but for this test we just want to verify the render_prompt flow works
    let args = {};
    let result = Greet$render_prompt("World");
    // If we got here without crashing, the call worked
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
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
    prompt `
        Hello, ${name}!
    `
}

function get_prompt() -> baml.llm.PromptAst {
    Greet$render_prompt("World")
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
        .expect("failed to render prompt for inline client shorthand");

    assert_eq!(result, prompt_ast_message("system", "Hello, World!"));
}

/// B-626: `<Fn>$render_prompt` renders offline and must NOT require the client's
/// `api_key` env var.
///
/// The client here reads `api_key` from `OPENAI_API_KEY_UNSET_B626`, a variable
/// that is never set. Before the fix, `render_prompt` eagerly constructed the
/// real primitive client (`Fast$new` → `baml.env.get_or_panic`) and panicked
/// with `UserPanic { "env var not found: ..." }` before rendering. Now the
/// render path constructs the client leniently (for its provider/role metadata
/// only), so the prompt renders without any credential set.
#[tokio::test]
async fn test_render_prompt_offline_without_api_key_env() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
class C { x: string }

client Fast {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key env.OPENAI_API_KEY_UNSET_B626
    }
}

function Extract(raw: string) -> C {
    client Fast
    prompt `
        Extract from ${raw}.
        ${ctx.output_format}
    `
}

function get_prompt() -> baml.llm.PromptAst {
    Extract$render_prompt("hello")
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
        .expect("render_prompt must succeed offline without the api_key env var");

    let rendered = common::prompt_ast_to_string(&result);
    assert!(
        rendered.contains("Extract from hello."),
        "rendered prompt should contain the interpolated arg, got: {rendered}"
    );
    assert!(
        rendered.contains("x: string"),
        "rendered prompt should contain the output-format schema, got: {rendered}"
    );
}

/// B-626 boundary: the offline `render_prompt` path tolerates a missing
/// credential env var, but the request-building path must NOT. With the same
/// unset `api_key` env var, `<Fn>$build_request` still constructs the client
/// strictly and surfaces the missing variable (as a `get_or_panic` `UserPanic`),
/// so we don't over-loosen and silently build an unauthenticated request.
#[tokio::test]
async fn test_build_request_still_requires_api_key_env() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
class C { x: string }

client Fast {
    provider openai
    options {
        model "gpt-4o-mini"
        api_key env.OPENAI_API_KEY_UNSET_B626
    }
}

function Extract(raw: string) -> C {
    client Fast
    prompt `
        Extract from ${raw}.
        ${ctx.output_format}
    `
}

function get_request() -> int {
    let request = Extract$build_request("hello");
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "get_request",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    let err = result.expect_err("build_request must still require the api_key env var");
    assert!(
        err.to_string().contains("OPENAI_API_KEY_UNSET_B626"),
        "error should name the missing env var, got: {err}"
    );
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
    prompt `
        Hello, ${name}!
    `
}

// Function that returns the PromptAst type - this should work since
// PromptAst is now a visible builtin type
function get_prompt() -> baml.llm.PromptAst {
    let args: map<string, unknown> = { "name": "World" };
    Greet$render_prompt("World")
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
        .await;

    match result {
        Ok(value) => {
            // Verify it's a PromptAst (wrapped in Adt)
            match &value {
                BexExternalValue::Instance {
                    class_name, fields, ..
                } => {
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
    prompt `
        Hello, ${name}!
    `
}

function test_build_request() -> int {
    let args: map<string, unknown> = { "name": "World" };
    let request = Greet$build_request("World");
    42
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
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
    prompt `
        Hello, ${name}!
    `
}

function test_call_llm() -> unknown {
    let args: map<string, unknown> = { "name": "World" };
    Greet("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
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
    prompt `
        Hello, ${name}!
    `
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "Greet",
            vec![BexExternalValue::String("World".to_string().into())],
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
    prompt `
        Hello, ${name}!
    `
}

function test_call_llm() -> string {
    Greet("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
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
    prompt `
        Hello, ${name}!
    `
}

function test_call_llm() -> unknown {
    let args: map<string, unknown> = { "name": "World" };
    Greet("World")
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
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
        type_args: vec![],
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
    prompt `
        Return whatever JSON you like.

        ${ctx.output_format}
    `
}

function get_prompt() -> baml.llm.PromptAst {
    ExtractAny$render_prompt()
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

    // Extract the rendered text from the PromptAst.
    let rendered_text = match &result {
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
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
