//! Integration tests for the LLM `render_prompt` flow.
//!
//! These tests verify that:
//! 1. backtick `prompt` closures render correctly against a hand-built `Context`
//!    (variable substitution, chat-role splitting)
//! 2. the `<Fn>$render_prompt` / `<Fn>$build_request` companions render new-mode
//!    (backtick) prompts, including `ctx.output_format`
//! 3. the orchestration entry points fail cleanly without a valid API key

use baml_builtins2::PromptAst as BuiltinPromptAst;
use bex_engine::FunctionCallContextBuilder;
use bex_heap::BexExternalValue;

/// A backtick `prompt` closure applied to a hand-built `Context` substitutes
/// string and int variables; a role-less prompt renders as plain text.
#[tokio::test]
async fn test_render_prompt_directly() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function render_text(name: string, age: int) -> string {
    let cc = baml.llm.ContextClient { name: "test", provider: "openai", default_role: "user", allowed_roles: ["user", "assistant", "system"] }
    let ctx = baml.llm.Context { client: cc, tags: {} }
    let render = prompt`Hello, ${name}! You are ${age} years old.`
    render(ctx).text()
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "render_text",
            vec![
                BexExternalValue::String("Alice".to_string().into()),
                BexExternalValue::Int(30),
            ],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("prompt closure render failed");

    assert_eq!(
        result,
        BexExternalValue::String("Hello, Alice! You are 30 years old.".to_string().into())
    );
}

/// `${role(...)}` markers split a backtick prompt into chat messages. Fold the
/// structured `.messages()` back into `role=content;` pairs so the assertion
/// pins both the split and the substituted content.
#[tokio::test]
async fn test_render_prompt_with_chat_roles() {
    use bex_engine::BexEngine;
    use sys_native::SysOpsExt;

    let source = r##"
function render_messages(question: string) -> string {
    let cc = baml.llm.ContextClient { name: "test", provider: "openai", default_role: "user", allowed_roles: ["user", "assistant", "system"] }
    let ctx = baml.llm.Context { client: cc, tags: {} }
    let render = prompt`${role("system")}You are a helpful assistant.${role("user")}${question}`
    let out = ""
    for (let m in render(ctx).messages()) {
        out += m.role + "=" + m.content + ";"
    }
    out
}
"##;

    let snapshot = common::compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    let result = engine
        .call_function(
            "render_messages",
            vec![BexExternalValue::String("What is 2+2?".to_string().into())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("prompt closure render failed");

    assert_eq!(
        result,
        BexExternalValue::String(
            "system=You are a helpful assistant.;user=What is 2+2?;"
                .to_string()
                .into()
        )
    );
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
        r#"render_null_as = "omit""#,
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
/// 2. Calls a BAML function that internally calls the `Greet$render_prompt` companion
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

// Test wrapper that calls the render_prompt companion and returns something we
// can check. Since PromptAst isn't a user-facing type, we just verify the call
// succeeds.
function test_render() -> int {
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
                    // The template "Hello, ${name}!" with name="World" should render to PromptAst::String
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
/// This test verifies the `Greet$build_request` companion is callable
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

// Returns Greet's concrete type: the closure path renders ${ctx.output_format}
// context from T, so T must be a data type (the generated companion passes the
// parent function's return type the same way).
function test_call_llm() -> string {
    let name = "World";
    let args: map<string, unknown> = { "name": name };
    // New-mode functions render through their prompt closure; pass one
    // explicitly, the same shape the generated companion body threads through.
    baml.llm.call_llm_function(TestClient, "Greet", args, prompt_closure = prompt`Hello, ${name}!`)
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

// Returns Greet's concrete type: the closure path renders ${ctx.output_format}
// context from T, so T must be a data type (the generated companion passes the
// parent function's return type the same way).
function test_call_llm() -> map<string, int> {
    let name = "World";
    let args: map<string, unknown> = { "name": name };
    // New-mode functions render through their prompt closure; pass one
    // explicitly, the same shape the generated companion body threads through.
    baml.llm.call_llm_function(TestClient, "Greet", args, prompt_closure = prompt`Hello, ${name}!`)
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
// Expression functions in prompts (formerly template_string Jinja macros)
// ============================================================================

/// Build a `BexExternalValue` wrapping a single-message `PromptAst`.
///
/// The engine renders a prompt without explicit `${role(...)}` markers as a
/// single `Message` with the client's default role ("system" for openai).
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

/// Test that a regular expression function (the new-mode replacement for
/// `template_string` Jinja macros) is callable from a backtick prompt.
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

function Greet(name: string) -> string {
    `Hello, ${name}!`
}

function TestFunc(name: string) -> string {
    client TestClient
    prompt `
        ${Greet(name)}
    `
}

function get_prompt() -> baml.llm.PromptAst {
    TestFunc$render_prompt("Alice")
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
        .expect("failed to render prompt that calls expression function Greet(name)");
    assert_eq!(result, prompt_ast_message("system", "Hello, Alice!"));
}

/// Test that nested expression-function calls expand correctly.
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

function Inner() -> string {
    `INNER`
}
function Outer() -> string {
    `before ${Inner()} after`
}

function TestFunc() -> string {
    client TestClient
    prompt `${Outer()}`
}

function get_prompt() -> baml.llm.PromptAst {
    TestFunc$render_prompt()
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
        .expect("failed to render prompt with nested expression functions Outer() -> Inner()");
    assert_eq!(result, prompt_ast_message("system", "before INNER after"));
}

/// Test an expression function with two args, one of which is a class (struct).
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

function Describe(label: string, person: Person) -> string {
    `${label}: ${person.name} (age ${person.age})`
}

function TestFunc(label: string, person: Person) -> string {
    client TestClient
    prompt `
        ${Describe(label, person)}
    `
}

function get_prompt() -> baml.llm.PromptAst {
    TestFunc$render_prompt("User", Person { name: "Bob", age: 42 })
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
        .expect("failed to render prompt with 2-arg expression function Describe(label, person)");
    assert_eq!(result, prompt_ast_message("system", "User: Bob (age 42)"));
}

/// Test that parameterless expression functions work.
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

function Header() -> string {
    `=== HEADER ===`
}

function TestFunc() -> string {
    client TestClient
    prompt `${Header()}
Content here`
}

function get_prompt() -> baml.llm.PromptAst {
    TestFunc$render_prompt()
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
        .expect("failed to render prompt that calls parameterless expression function Header()");
    assert_eq!(
        result,
        prompt_ast_message("system", "=== HEADER ===\nContent here")
    );
}

// ============================================================================
// Phase 3: json alias LLM-path sentinel
// ============================================================================

/// Verify that a `function F() -> json` whose prompt renders `${ctx.output_format}`
/// produces a prompt containing "Respond with valid JSON." - the static literal
/// required by BEP-038 Phase 3 - and does NOT contain the union-arm enumeration
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
