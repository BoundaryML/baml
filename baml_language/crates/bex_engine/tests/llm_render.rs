//! Integration tests for LLM prompt rendering.
//!
//! Tests drive the compiler-generated `<Fn>$render_prompt` companion, which
//! renders the spec's prompt (with the return type's output format) as a
//! structural `ai.Prompt`.

use bex_engine::FunctionCallContextBuilder;
use bex_heap::BexExternalValue;

mod common;

use common::{EngineProgram, assert_engine_executes};

#[tokio::test]
async fn backtick_prompt_interpolates_arguments() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
function Greet(name: string, age: int) -> string {
    client: "openai/gpt-4o"
    prompt: `Hello, ${name}! You are ${age} years old.`
}

function main() -> string {
    Greet$render_prompt("Alice", 30).text()
}
"#,
        entry: "main",
        expected: Ok(BexExternalValue::from(
            "Hello, Alice! You are 30 years old.",
        )),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_prompt_builds_chat_roles() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
function Answer(question: string) -> string {
    client: "openai/gpt-4o"
    prompt: `${role("system")}You are a helpful assistant.${role("user")}${question}`
}

function main() -> string {
    let out = ""
    for (let message in Answer$render_prompt("What is 2+2?").messages()) {
        out += message.role + "=" + message.content + ";"
    }
    out
}
"#,
        entry: "main",
        expected: Ok(BexExternalValue::from(
            "system=You are a helpful assistant.;user=What is 2+2?;",
        )),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn backtick_prompt_interpolates_enum_values() -> anyhow::Result<()> {
    assert_engine_executes(EngineProgram {
        source: r#"
enum Category {
    SPORTS
    TECH
}

function Categorize(category: Category) -> string {
    client: "openai/gpt-4o"
    prompt: `Category: ${category}`
}

function main() -> string {
    Categorize$render_prompt(Category.SPORTS).text()
}
"#,
        entry: "main",
        expected: Ok(BexExternalValue::from("Category: SPORTS")),
        ..Default::default()
    })
    .await
}

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

client Fast = openai.ResponsesClient.new(model = "gpt-4o-mini");

function Extract(raw: string) -> C {
    client: Fast
    prompt: `
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
    client: "openai/gpt-4o-mini"
    prompt: `Hello, ${name}!`
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
    client: "openai/gpt-4o"
    prompt: `
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
