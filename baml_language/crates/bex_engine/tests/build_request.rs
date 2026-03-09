//! End-to-end tests for `baml.llm.build_request` (OpenAI + Anthropic).
//!
//! Each test compiles BAML source, calls `baml.llm.build_request(...)`,
//! and asserts the exact JSON body schema of the resulting HTTP request.

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

/// Create a BexExternalValue for a media object.
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

/// Shared OpenAI client block for media tests.
const OPENAI_CLIENT: &str = r#"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-test" }
}
"#;

// ============================================================================
// Schema shape tests — verify the JSON body structure
// ============================================================================

#[tokio::test]
async fn test_openai_single_user_message_schema() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4o"
        api_key "sk-test"
    }
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
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello, World!"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_openai_system_and_user_messages_schema() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4-turbo"
        api_key "sk-test"
    }
}
function F(question: string) -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        You are a helpful assistant.
        {{ _.role("user") }}
        {{ question }}
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", { "question": "What is 2+2?" }).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4-turbo",
            "messages": [
                {
                    "role": "system",
                    "content": [{"type": "text", "text": "You are a helpful assistant."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "What is 2+2?"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_openai_options_forwarded_to_body() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4o"
        api_key "sk-test"
        temperature 0.5
        max_tokens 100
        top_p 0.9
    }
}
function F() -> string {
    client C
    prompt #"hi"#
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
            "temperature": 0.5,
            "max_tokens": 100,
            "top_p": 0.9,
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "hi"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_openai_internal_options_excluded_from_body() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4o"
        api_key "sk-secret"
        base_url "https://api.openai.com"
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    // api_key and base_url must NOT leak into the body
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "hi"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_openai_no_model_when_absent() {
    let source = r##"
client C {
    provider openai
    options {
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "hi"}]
                }
            ]
        })
    );
}

// ============================================================================
// URL + method + headers
// ============================================================================

#[tokio::test]
async fn test_openai_url_and_method() {
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
    prompt #"hi"#
}
function get_url() -> string {
    baml.llm.build_request("F", {}).url
}
function get_method() -> string {
    baml.llm.build_request("F", {}).method
}
"##;

    let url = run_baml(source, "get_url").await;
    assert_eq!(
        as_string(&url),
        "https://api.openai.com/v1/chat/completions"
    );

    let method = run_baml(source, "get_method").await;
    assert_eq!(as_string(&method), "POST");
}

#[tokio::test]
async fn test_openai_custom_base_url() {
    let source = r##"
client C {
    provider openai
    options {
        model "gpt-4o"
        base_url "https://custom.api.com"
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_url() -> string {
    baml.llm.build_request("F", {}).url
}
"##;

    let url = run_baml(source, "get_url").await;
    assert_eq!(
        as_string(&url),
        "https://custom.api.com/v1/chat/completions"
    );
}

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
                    "role": "user",
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
                    "role": "user",
                    "content": [{"type": "text", "text": "Bob is 42"}]
                }
            ]
        })
    );
}

// ============================================================================
// OpenAI media tests — image, audio, pdf, video
// ============================================================================

#[tokio::test]
async fn test_openai_image_url() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(img: image) -> string {{
    client C
    prompt #"{{{{ img }}}}"#
}}
function get_body(img: image) -> string {{
    baml.llm.build_request("F", {{ "img": img }}).body
}}
"##
    );
    let img = media_value(
        MediaKind::Image,
        MediaContent::Url {
            url: "https://example.com/cat.png".into(),
            base64_data: None,
        },
        Some("image/png"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![img]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}})
    );
}

#[tokio::test]
async fn test_openai_image_base64() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(img: image) -> string {{
    client C
    prompt #"{{{{ img }}}}"#
}}
function get_body(img: image) -> string {{
    baml.llm.build_request("F", {{ "img": img }}).body
}}
"##
    );
    let img = media_value(
        MediaKind::Image,
        MediaContent::Base64 {
            base64_data: "iVBORw0KGgo=".into(),
        },
        Some("image/png"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![img]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}})
    );
}

#[tokio::test]
async fn test_openai_audio_base64() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(a: audio) -> string {{
    client C
    prompt #"{{{{ a }}}}"#
}}
function get_body(a: audio) -> string {{
    baml.llm.build_request("F", {{ "a": a }}).body
}}
"##
    );
    let audio = media_value(
        MediaKind::Audio,
        MediaContent::Base64 {
            base64_data: "AAAA".into(),
        },
        Some("audio/wav"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![audio]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({"type": "input_audio", "input_audio": {"data": "AAAA", "format": "wav"}})
    );
}

#[tokio::test]
async fn test_openai_audio_mpeg_becomes_mp3() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(a: audio) -> string {{
    client C
    prompt #"{{{{ a }}}}"#
}}
function get_body(a: audio) -> string {{
    baml.llm.build_request("F", {{ "a": a }}).body
}}
"##
    );
    let audio = media_value(
        MediaKind::Audio,
        MediaContent::Base64 {
            base64_data: "AAAA".into(),
        },
        Some("audio/mpeg"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![audio]).await);
    assert_eq!(
        body["messages"][0]["content"][0]["input_audio"]["format"],
        "mp3"
    );
}

#[tokio::test]
async fn test_openai_audio_url() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(a: audio) -> string {{
    client C
    prompt #"{{{{ a }}}}"#
}}
function get_body(a: audio) -> string {{
    baml.llm.build_request("F", {{ "a": a }}).body
}}
"##
    );
    let audio = media_value(
        MediaKind::Audio,
        MediaContent::Url {
            url: "https://example.com/speech.wav".into(),
            base64_data: None,
        },
        Some("audio/wav"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![audio]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({"type": "input_audio", "input_audio": {"data": "https://example.com/speech.wav", "format": "wav"}})
    );
}

#[tokio::test]
async fn test_openai_pdf_url() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(doc: pdf) -> string {{
    client C
    prompt #"{{{{ doc }}}}"#
}}
function get_body(doc: pdf) -> string {{
    baml.llm.build_request("F", {{ "doc": doc }}).body
}}
"##
    );
    let pdf = media_value(
        MediaKind::Pdf,
        MediaContent::Url {
            url: "https://example.com/doc.pdf".into(),
            base64_data: None,
        },
        Some("application/pdf"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![pdf]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({
            "type": "file",
            "file": {
                "file_url": "https://example.com/doc.pdf",
                "filename": "document.pdf"
            }
        })
    );
}

#[tokio::test]
async fn test_openai_pdf_base64() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(doc: pdf) -> string {{
    client C
    prompt #"{{{{ doc }}}}"#
}}
function get_body(doc: pdf) -> string {{
    baml.llm.build_request("F", {{ "doc": doc }}).body
}}
"##
    );
    let pdf = media_value(
        MediaKind::Pdf,
        MediaContent::Base64 {
            base64_data: "JVBERi0=".into(),
        },
        Some("application/pdf"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![pdf]).await);
    assert_eq!(
        body["messages"][0]["content"][0],
        serde_json::json!({
            "type": "file",
            "file": {
                "file_data": "data:application/pdf;base64,JVBERi0=",
                "filename": "document.pdf"
            }
        })
    );
}

#[tokio::test]
async fn test_openai_video_unsupported() {
    let source = format!(
        r##"
{OPENAI_CLIENT}
function F(v: video) -> string {{
    client C
    prompt #"{{{{ v }}}}"#
}}
function get_body(v: video) -> string {{
    baml.llm.build_request("F", {{ "v": v }}).body
}}
"##
    );
    let video = media_value(
        MediaKind::Video,
        MediaContent::Url {
            url: "https://example.com/clip.mp4".into(),
            base64_data: None,
        },
        Some("video/mp4"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![video]).await);
    let part = &body["messages"][0]["content"][0];
    assert_eq!(part["type"], "text");
    assert!(part["text"].as_str().unwrap().contains("unsupported"));
}

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
    let content = body["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    // Note: parse_message_content trims whitespace from text chunks
    assert_eq!(
        content[0],
        serde_json::json!({"type": "text", "text": "What is in this image?"})
    );
    assert_eq!(
        content[1],
        serde_json::json!({"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}})
    );
}

// ============================================================================
// Azure OpenAI — max_tokens default
// ============================================================================

#[tokio::test]
async fn test_azure_defaults_max_tokens_4096() {
    let source = r##"
client C {
    provider azure-openai
    options {
        model "gpt-4o"
        resource_name "my-resource"
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(body["max_tokens"], 4096);
}

#[tokio::test]
async fn test_azure_no_default_when_max_tokens_set() {
    let source = r##"
client C {
    provider azure-openai
    options {
        model "gpt-4o"
        resource_name "my-resource"
        api_key "sk-test"
        max_tokens 1000
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(body["max_tokens"], 1000);
}

#[tokio::test]
async fn test_azure_no_default_when_max_completion_tokens_set() {
    let source = r##"
client C {
    provider azure-openai
    options {
        model "gpt-4o"
        resource_name "my-resource"
        api_key "sk-test"
        max_completion_tokens 2000
    }
}
function F() -> string {
    client C
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert!(body.get("max_tokens").is_none());
    assert_eq!(body["max_completion_tokens"], 2000);
}

#[tokio::test]
async fn test_openai_no_default_max_tokens() {
    // Non-Azure OpenAI should NOT get a default max_tokens
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
    prompt #"hi"#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert!(body.get("max_tokens").is_none());
}

// ============================================================================
// O1/O3 model restrictions — system messages converted to user
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
    let messages = body["messages"].as_array().unwrap();
    // Both messages should be "user" — system is not allowed on o1
    for msg in messages {
        assert_eq!(msg["role"], "user", "o1 should not have system messages");
    }
}

#[tokio::test]
async fn test_o1_mini_converts_system_to_user() {
    let source = r##"
client C {
    provider openai
    options {
        model "o1-mini"
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        Be concise.
        {{ _.role("user") }}
        What is 2+2?
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    let messages = body["messages"].as_array().unwrap();
    for msg in messages {
        assert_eq!(
            msg["role"], "user",
            "o1-mini should not have system messages"
        );
    }
}

#[tokio::test]
async fn test_o3_converts_system_to_user() {
    let source = r##"
client C {
    provider openai
    options {
        model "o3-mini"
        api_key "sk-test"
    }
}
function F() -> string {
    client C
    prompt #"
        {{ _.role("system") }}
        System prompt.
        {{ _.role("user") }}
        User prompt.
    "#
}
function get_body() -> string {
    baml.llm.build_request("F", {}).body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    let messages = body["messages"].as_array().unwrap();
    for msg in messages {
        assert_eq!(
            msg["role"], "user",
            "o3-mini should not have system messages"
        );
    }
}

#[tokio::test]
async fn test_non_o_series_keeps_system() {
    // Regular GPT models should keep system messages
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
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "user");
}

// ============================================================================
// OpenAI Responses API — uses "input" key with input_text/output_text types
// ============================================================================

const RESPONSES_CLIENT: &str = r#"
client C {
    provider openai-responses
    options { model "gpt-4o"  api_key "sk-test" }
}
"#;

#[tokio::test]
async fn test_responses_api_basic() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F(name: string) -> string {{
    client C
    prompt #"Hello, {{{{ name }}}}!"#
}}
function get_body() -> string {{
    baml.llm.build_request("F", {{ "name": "World" }}).body
}}
"##
    );

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "input": [
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Hello, World!"}]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_responses_api_system_and_user() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F() -> string {{
    client C
    prompt #"
        {{{{ _.role("system") }}}}
        You are helpful.
        {{{{ _.role("user") }}}}
        Hi
    "#
}}
function get_body() -> string {{
    baml.llm.build_request("F", {{}}).body
}}
"##
    );

    let body = body_json(&run_baml(&source, "get_body").await);
    let input = body["input"].as_array().unwrap();
    assert_eq!(input[0]["role"], "system");
    assert_eq!(input[0]["content"][0]["type"], "input_text");
    assert_eq!(input[1]["role"], "user");
    assert_eq!(input[1]["content"][0]["type"], "input_text");
}

#[tokio::test]
async fn test_responses_api_url() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F() -> string {{
    client C
    prompt #"hi"#
}}
function get_url() -> string {{
    baml.llm.build_request("F", {{}}).url
}}
"##
    );

    let url = run_baml(&source, "get_url").await;
    assert_eq!(as_string(&url), "https://api.openai.com/v1/responses");
}

#[tokio::test]
async fn test_responses_api_image_url() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F(img: image) -> string {{
    client C
    prompt #"{{{{ img }}}}"#
}}
function get_body(img: image) -> string {{
    baml.llm.build_request("F", {{ "img": img }}).body
}}
"##
    );
    let img = media_value(
        MediaKind::Image,
        MediaContent::Url {
            url: "https://example.com/cat.png".into(),
            base64_data: None,
        },
        Some("image/png"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![img]).await);
    assert_eq!(
        body["input"][0]["content"][0],
        serde_json::json!({
            "type": "input_image",
            "detail": "auto",
            "image_url": "https://example.com/cat.png"
        })
    );
}

#[tokio::test]
async fn test_responses_api_pdf_url() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F(doc: pdf) -> string {{
    client C
    prompt #"{{{{ doc }}}}"#
}}
function get_body(doc: pdf) -> string {{
    baml.llm.build_request("F", {{ "doc": doc }}).body
}}
"##
    );
    let pdf = media_value(
        MediaKind::Pdf,
        MediaContent::Url {
            url: "https://example.com/doc.pdf".into(),
            base64_data: None,
        },
        Some("application/pdf"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![pdf]).await);
    assert_eq!(
        body["input"][0]["content"][0],
        serde_json::json!({
            "type": "input_file",
            "file_url": "https://example.com/doc.pdf",
            "filename": "document.pdf"
        })
    );
}

#[tokio::test]
async fn test_responses_api_audio_base64() {
    let source = format!(
        r##"
{RESPONSES_CLIENT}
function F(a: audio) -> string {{
    client C
    prompt #"{{{{ a }}}}"#
}}
function get_body(a: audio) -> string {{
    baml.llm.build_request("F", {{ "a": a }}).body
}}
"##
    );
    let audio = media_value(
        MediaKind::Audio,
        MediaContent::Base64 {
            base64_data: "AAAA".into(),
        },
        Some("audio/wav"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![audio]).await);
    assert_eq!(
        body["input"][0]["content"][0],
        serde_json::json!({
            "type": "input_audio",
            "input_audio": {"data": "AAAA", "format": "wav"}
        })
    );
}
