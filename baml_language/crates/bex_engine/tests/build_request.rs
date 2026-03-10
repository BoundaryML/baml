//! Integration tests for `baml.llm.build_request` (`OpenAI` + Anthropic).
//!
//! These tests exercise the full pipeline: BAML source -> compile -> engine ->
//! `baml.llm.build_request(...)`. They cover behaviors that require the full
//! compilation + specialization pipeline (template strings, struct args,
//! o-series model restrictions, media passed through BAML args).
//!
//! Unit-level tests for JSON body shapes, URL building, media conversion, and
//! option forwarding live in `sys_llm::build_request::openai::tests` and
//! `sys_llm::build_request::mod::tests`.

mod common;

use std::sync::Arc;

use baml_builtins::{MediaContent, MediaValue};
use baml_type::MediaKind;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_external_types::BexExternalAdt;
use sys_native::SysOpsExt;

/// Helper: compile source, run `entry` with no args, return the result.
async fn run_baml(source: &str, entry: &str) -> BexExternalValue {
    run_baml_with_args(source, entry, vec![]).await
}

/// Helper: compile source, run `entry` with given args, return the result.
async fn run_baml_with_args(
    source: &str,
    entry: &str,
    args: Vec<BexExternalValue>,
) -> BexExternalValue {
    let snapshot = common::compile_for_engine(source);
    let engine = BexEngine::new(snapshot, sys_types::SysOps::native().into(), None)
        .expect("Failed to create engine");

    engine
        .call_function(
            entry,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .unwrap_or_else(|e| panic!("{entry} failed: {e}"))
}

/// Create a `BexExternalValue` for a media object.
fn media_value(
    kind: MediaKind,
    content: MediaContent,
    mime_type: Option<&str>,
) -> BexExternalValue {
    BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(MediaValue::new(
        kind,
        content,
        mime_type.map(String::from),
    ))))
}

fn as_string(val: &BexExternalValue) -> &str {
    match val {
        BexExternalValue::String(s) => s.as_str(),
        other => panic!("expected String, got {other:?}"),
    }
}

fn body_json(val: &BexExternalValue) -> serde_json::Value {
    let s = as_string(val);
    serde_json::from_str(s).unwrap_or_else(|e| panic!("invalid JSON: {e}\nbody: {s}"))
}

/// Shared `OpenAI` client block.
const OPENAI_CLIENT: &str = r#"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
"#;

// ============================================================================
// Template strings — verify they expand before request building
// ============================================================================

#[tokio::test]
async fn test_openai_template_string_expansion() {
    let source = r##"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
template_string Greet(name: string) #"Hello, {{ name }}!"#
function F(name: string) -> string {
    client C
    prompt #"{{ Greet(name) }}"#
}
function get_body() -> string {
    baml.llm.build_request("F", { "name": "Alice" }).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "Hello, Alice!"}]
                }
            ]
        })
    );
}

// ============================================================================
// Struct args — verify they render into the prompt correctly
// ============================================================================

#[tokio::test]
async fn test_openai_struct_arg_in_prompt() {
    let source = r##"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
class Person {
    name string
    age int
}
function F(p: Person) -> string {
    client C
    prompt #"{{ p.name }} is {{ p.age }}"#
}
function get_body() -> string {
    baml.llm.build_request("F", { "p": { "name": "Bob", "age": 42 } }).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "Bob is 42"}]
                }
            ]
        })
    );
}

// ============================================================================
// O1/O3 model restrictions — system messages converted to user
// (requires the full specialize_prompt pipeline)
// ============================================================================

#[tokio::test]
async fn test_o1_converts_system_to_user() {
    let source = r##"
client C {
    provider openai
    options {
        model "o1"
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        You are a helpful assistant.
        {{ _.role("user") }}
        Hello
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "o1",
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "You are a helpful assistant."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_non_o_series_keeps_system() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4o"
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        You are helpful.
        {{ _.role("user") }}
        Hi
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "You are helpful."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hi"}]
                }
            ]
        })
    );
}

// ============================================================================
// Multi-message conversations (3+ messages)
// ============================================================================

#[tokio::test]
async fn test_openai_three_role_conversation() {
    let source = r##"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        You are a helpful assistant.
        {{ _.role("user") }}
        What is 2+2?
        {{ _.role("assistant") }}
        4
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "You are a helpful assistant."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "What is 2+2?"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "4"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_openai_multi_turn_conversation() {
    let source = r##"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        Be concise.
        {{ _.role("user") }}
        Hello
        {{ _.role("assistant") }}
        Hi!
        {{ _.role("user") }}
        How are you?
        {{ _.role("assistant") }}
        Good, thanks!
        {{ _.role("user") }}
        Goodbye
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "Be concise."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Hi!"}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "How are you?"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Good, thanks!"}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Goodbye"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_responses_api_multi_turn() {
    let source = r##"
client C {
    provider openai-responses
    options { model "gpt-4o"  api_key "sk-test" }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        You are helpful.
        {{ _.role("user") }}
        Hi
        {{ _.role("assistant") }}
        Hello!
        {{ _.role("user") }}
        Bye
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "input": [
                {
                    "role": "system",
                    "content": [{"type": "input_text", "text": "You are helpful."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Hi"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "Hello!"}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Bye"}]
                }
            ]
        })
    );
}

// ============================================================================
// Media passed through BAML function args (smoke tests — detailed coverage
// in sys_llm unit tests)
// ============================================================================

#[tokio::test]
async fn test_openai_mixed_text_and_image() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(img: image) -> string {{
    client C
    prompt #"What is in this image? {{{{ img }}}}"#
}}
function get_body(img: image) -> string {{
    baml.llm.build_request("F", {{ "img": img }}).body
}}
"##
    );
    let img = media_value(
        MediaKind::Image,
        MediaContent::Url {
            url: "https://example.com/photo.jpg".into(),
            base64_data: None,
        },
        Some("image/jpeg"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![img]).await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [
                        {"type": "text", "text": "What is in this image?"},
                        {"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}}
                    ]
                }
            ]
        })
    );
}

// ============================================================================
// OpenAI Responses API — smoke test for full pipeline
// ============================================================================

#[tokio::test]
async fn test_responses_api_basic() {
    let source = r##"
client C {
    provider openai-responses
    options { model "gpt-4o"  api_key "sk-test" }
}
function F(name: string) -> string {
    client C
    prompt #"Hello, {{ name }}!"#
}
function get_body() -> string {
    baml.llm.build_request("F", { "name": "World" }).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "input": [
                {
                    "role": "system",
                    "content": [{"type": "input_text", "text": "Hello, World!"}]
                }
            ]
        })
    );
}
