//! Integration tests for `baml.llm.build_request` (`OpenAI` + Anthropic).
//!
//! These tests exercise the full pipeline: BAML source -> compile -> engine ->
//! the generated `F$build_request` companion (the playground path, which
//! carries the compiled prompt closure). They cover behaviors that require the
//! full compilation + specialization pipeline (prompt helper expansion, struct
//! args, o-series model restrictions, media passed through BAML args).
//!
//! Unit-level tests for JSON body shapes, URL building, media conversion, and
//! option forwarding live in `sys_llm::build_request::openai::tests` and
//! `sys_llm::build_request::mod::tests`.

// The B-489 api_key-defaulting tests set `OPENAI_API_KEY` via `std::env::set_var`
// (nextest isolates each test in its own process).
#![allow(unsafe_code)]

mod common;

use std::sync::Arc;

use baml_builtins2::{MediaContent, MediaValue};
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
    let engine = Arc::new(
        BexEngine::new(snapshot, sys_native::SysOps::native().into(), Vec::new())
            .expect("Failed to create engine"),
    );

    engine
        .call_function(
            entry,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
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

/// Shared `OpenAI` Responses API client block.
const OPENAI_RESPONSES_CLIENT: &str = r#"
client C {
    provider openai-responses
    options { model "gpt-4o"  api_key "sk-test" }
}
"#;

/// Shared Vercel AI Gateway Images API client block.
const AI_GATEWAY_IMAGES_CLIENT: &str = r#"
client C {
    provider ai-gateway-images
    options {
        model "bfl/flux-2-pro"
        api_key "sk-test"
        n 2
        providerOptions {
            blackForestLabs {
                outputFormat "jpeg"
            }
        }
    }
}
"#;

/// Shared `OpenAI` O1 client block.
const OPENAI_O1_CLIENT: &str = r#"
client C {
    provider openai
    options { model "o1"  api_key "sk-test" }
}
"#;

// ============================================================================
// Prompt helper functions (template_string replacement) — verify they
// expand before request building
// ============================================================================

#[tokio::test]
async fn test_openai_prompt_helper_expansion() {
    let source = [
        OPENAI_CLIENT,
        r##"
function Greet(name: string) -> string {
    `Hello, ${name}!`
}
function F(name: string) -> string {
    client C
    prompt `${Greet(name)}`
}
function get_body() -> string {
    F$build_request("Alice").body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
// Named client api_key defaulting (B-489)
//
// A named `client<llm>` with `provider openai` and no explicit `api_key` must
// default the key from the provider's conventional env var (`OPENAI_API_KEY`),
// exactly as the `"openai/model"` inline shorthand does. An explicit `api_key`
// in `options` still wins.
//
// These tests read the `authorization` header on the built request rather than
// the body (auth headers are where the key lands). They rely on nextest's
// process-per-test isolation for the `OPENAI_API_KEY` env var; both write the
// same value so a shared-process `cargo test` run cannot observe a torn value.
// ============================================================================

const OPENAI_ENV_KEY: &str = "sk-from-env-b489";

/// Read the `authorization` header off the request `F$build_request` builds.
async fn openai_authorization_header(source: &str) -> String {
    let full = [
        source,
        r##"
function F(name: string) -> string {
    client C
    prompt `Hello, ${name}!`
}
function get_auth() -> string {
    F$build_request("Alice").headers.get("authorization") ?? "MISSING"
}
"##,
    ]
    .join("\n");
    let result = run_baml(&full, "get_auth").await;
    as_string(&result).to_string()
}

#[tokio::test]
async fn test_named_openai_client_defaults_api_key_from_env() {
    // SAFETY: nextest runs each test in its own process; other tests in this
    // binary set `api_key` explicitly and never read `OPENAI_API_KEY`.
    unsafe { std::env::set_var("OPENAI_API_KEY", OPENAI_ENV_KEY) };

    // Named client, provider openai, NO explicit api_key.
    let auth = openai_authorization_header(
        r#"
client C {
    provider openai
    options { model "gpt-4o" }
}
"#,
    )
    .await;

    assert_eq!(
        auth,
        format!("Bearer {OPENAI_ENV_KEY}"),
        "named openai client with no api_key should default to OPENAI_API_KEY"
    );
}

#[tokio::test]
async fn test_named_openai_client_explicit_api_key_overrides_env() {
    // SAFETY: see sibling test. Same value written so a shared-process run
    // cannot race on the value.
    unsafe { std::env::set_var("OPENAI_API_KEY", OPENAI_ENV_KEY) };

    // Explicit api_key must win over the env-var default.
    let auth = openai_authorization_header(
        r#"
client C {
    provider openai
    options { model "gpt-4o"  api_key "sk-explicit" }
}
"#,
    )
    .await;

    assert_eq!(
        auth, "Bearer sk-explicit",
        "explicit api_key must override the OPENAI_API_KEY default"
    );
}

// ============================================================================
// New-mode (backtick) prompts — the render_prompt / build_request companions
// must render through the compiled prompt closure, exactly like execution.
// Regression: previously these went through the Jinja-only path and rendered
// nothing for new-mode functions (empty playground preview / cURL).
// ============================================================================

// A new-mode LLM function `F` + a helper that yields its client value, so a test
// can call the `F$render_prompt` / `F$build_request` companions exactly like the
// host (the appended `client` param has no default applied by `call_function`).
const NEW_MODE_FN: &str = r#"
function F(name: string) -> string {
    client C
    prompt `Hello, ${name}!`
}
function client_value() -> baml.llm.Client { C }
"#;

#[tokio::test]
async fn test_openai_new_mode_render_prompt_companion() {
    let source = [OPENAI_CLIENT, NEW_MODE_FN].join("\n");
    let client = run_baml(&source, "client_value").await;

    // The playground preview calls the generated `F$render_prompt` companion.
    let prompt = run_baml_with_args(
        &source,
        "F$render_prompt",
        vec![BexExternalValue::String("Alice".into()), client],
    )
    .await;
    let text = common::prompt_ast_to_string(&prompt);
    assert!(
        text.contains("Hello, Alice!"),
        "new-mode render_prompt companion should render the backtick prompt, got: {text:?}"
    );
}

#[tokio::test]
async fn test_openai_new_mode_build_request_companion() {
    let source = [OPENAI_CLIENT, NEW_MODE_FN].join("\n");
    let client = run_baml(&source, "client_value").await;

    // The playground "generate cURL" calls the generated `F$build_request`
    // companion; its body must include the rendered prompt.
    let req = run_baml_with_args(
        &source,
        "F$build_request",
        vec![BexExternalValue::String("Alice".into()), client],
    )
    .await;
    let body = match &req {
        BexExternalValue::Instance { fields, .. } => match fields.get("body") {
            Some(BexExternalValue::String(s)) => s.to_string(),
            other => panic!("expected Request.body string, got {other:?}"),
        },
        other => panic!("expected http.Request instance, got {other:?}"),
    };
    assert!(
        body.contains("Hello, Alice!"),
        "new-mode build_request companion should render the backtick prompt into the body, got: {body}"
    );
}

// ============================================================================
// Struct args — verify they render into the prompt correctly
// ============================================================================

#[tokio::test]
async fn test_openai_struct_arg_in_prompt() {
    let source = [
        OPENAI_CLIENT,
        r##"
class Person {
    name string
    age int
}
function F(p: Person) -> string {
    client C
    prompt `${p.name} is ${p.age}`
}
function get_body() -> string {
    let p = Person { name: "Bob", age: 42 };
    F$build_request(p).body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
#[ignore = "o1 system-to-user conversion not yet working"]
async fn test_o1_converts_system_to_user() {
    let source = [
        OPENAI_O1_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are a helpful assistant.${role("user")}Hello`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "o1",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "You are a helpful assistant."},
                        {"type": "text", "text": "Hello"}
                    ]
                }
            ]
        })
    );
}

#[tokio::test]
async fn test_non_o_series_keeps_system() {
    let source = [
        OPENAI_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
    let source = [
        OPENAI_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are a helpful assistant.${role("user")}What is 2+2?${role("assistant")}4`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
    let source = [
        OPENAI_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}Be concise.${role("user")}Hello${role("assistant")}Hi!${role("user")}How are you?${role("assistant")}Good, thanks!${role("user")}Goodbye`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
    let source = [
        OPENAI_RESPONSES_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("user")}Hi${role("assistant")}Hello!${role("user")}Bye`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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
//
// Still on legacy Jinja prompt syntax: the BEP-049 structural assembler
// renders media interpolations as empty text (`prompt_value_text` in
// `sys_ops` handles scalars only; media is deferred per its M5d comment),
// so `${img}` cannot express what `{{ img }}` renders here yet.
// ============================================================================

#[tokio::test]
#[ignore = "media values cannot be converted to VM values yet"]
async fn test_openai_mixed_text_and_image() {
    let source = [
        OPENAI_CLIENT,
        r##"
function F(img: image) -> string {
    client C
    prompt #"What is in this image? {{ img }}"#
}
function get_body(img: image) -> string {
    baml.llm.build_request(C, "F", { "img": img }).body
}
"##,
    ]
    .join("\n");
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
    let source = [
        OPENAI_RESPONSES_CLIENT,
        r##"
function F(name: string) -> string {
    client C
    prompt `Hello, ${name}!`
}
function get_body() -> string {
    F$build_request("World").body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
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

#[tokio::test]
async fn test_ai_gateway_images_builds_image_model_body() {
    let source = [
        AI_GATEWAY_IMAGES_CLIENT,
        r##"
function F() -> image {
    client C
    prompt `Draw a brass desk lamp on a walnut table.`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "prompt": "Draw a brass desk lamp on a walnut table.",
            "n": 2,
            "providerOptions": {
                "blackForestLabs": {
                    "outputFormat": "jpeg"
                }
            }
        })
    );
}

#[tokio::test]
async fn test_ai_gateway_images_uses_vercel_gateway_image_model_endpoint() {
    let source = [
        AI_GATEWAY_IMAGES_CLIENT,
        r##"
function F() -> image {
    client C
    prompt `Draw a brass desk lamp.`
}
function get_url() -> string {
    F$build_request().url
}
"##,
    ]
    .join("\n");

    let result = run_baml(&source, "get_url").await;
    assert_eq!(
        as_string(&result),
        "https://ai-gateway.vercel.sh/v4/ai/image-model"
    );
}

#[tokio::test]
async fn test_responses_api_image_output_enables_image_generation_tool() {
    let source = [
        OPENAI_RESPONSES_CLIENT,
        r##"
function F() -> image {
    client C
    prompt `Generate a square logo of a brass desk lamp.`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body["tools"],
        serde_json::json!([{ "type": "image_generation" }])
    );
    assert_eq!(
        body["tool_choice"],
        serde_json::json!({ "type": "image_generation" })
    );
}

#[tokio::test]
async fn test_responses_api_image_array_output_enables_image_generation_tool() {
    let source = [
        OPENAI_RESPONSES_CLIENT,
        r##"
function F() -> image[] {
    client C
    prompt `Generate two product photos of a brass desk lamp.`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body["tools"],
        serde_json::json!([{ "type": "image_generation" }])
    );
    assert_eq!(
        body["tool_choice"],
        serde_json::json!({ "type": "image_generation" })
    );
}

#[tokio::test]
async fn test_responses_api_mixed_text_image_output_enables_tool_and_choice() {
    let source = [
        OPENAI_RESPONSES_CLIENT,
        r##"
function F() -> (string | image)[] {
    client C
    prompt `Return a caption, then generate an illustration of a brass desk lamp.`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body["tools"],
        serde_json::json!([{ "type": "image_generation" }])
    );
    assert_eq!(
        body["tool_choice"],
        serde_json::json!({ "type": "image_generation" })
    );
}

// ============================================================================
// OpenAI multiple system messages — merged into one message
// ============================================================================

#[tokio::test]
#[ignore = "merge_adjacent_roles concatenates content instead of keeping separate parts"]
async fn test_openai_multiple_system_messages() {
    let source = [
        OPENAI_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("system")}You are concise.${role("user")}Hello`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "system",
                    "content": [
                        {"type": "text", "text": "You are helpful."},
                        {"type": "text", "text": "You are concise."}
                    ]
                },
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello"}]
                }
            ]
        })
    );
}

// ============================================================================
// Anthropic Integration Tests
// ============================================================================

/// Shared Anthropic client block.
const ANTHROPIC_CLIENT: &str = r#"
client C {
    provider anthropic
    options {
        model "claude-3-5-sonnet-20241022"
        api_key "sk-ant-test"
        default_role "user"
    }
}
"#;

// ============================================================================
// Prompt helper functions (template_string replacement) — verify they
// expand before request building
// ============================================================================

#[tokio::test]
#[ignore = "anthropic build_request missing max_tokens"]
async fn test_anthropic_prompt_helper_expansion() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function Greet(name: string) -> string {
    `Hello, ${name}!`
}
function F(name: string) -> string {
    client C
    prompt `${Greet(name)}`
}
function get_body() -> string {
    F$build_request("Alice").body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
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
#[ignore = "anthropic build_request missing max_tokens"]
async fn test_anthropic_struct_arg_in_prompt() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
class Person {
    name string
    age int
}
function F(p: Person) -> string {
    client C
    prompt `${p.name} is ${p.age}`
}
function get_body() -> string {
    let p = Person { name: "Bob", age: 42 };
    F$build_request(p).body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
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
// Multi-message conversations (3+ messages)
// ============================================================================

#[tokio::test]
#[ignore = "anthropic build_request missing max_tokens"]
async fn test_anthropic_three_role_conversation() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are a helpful assistant.${role("user")}What is 2+2?${role("assistant")}4`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
            "system": [
                {"type": "text", "text": "You are a helpful assistant."}
            ],
            "messages": [
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
#[ignore = "anthropic build_request missing max_tokens"]
async fn test_anthropic_multi_turn_conversation() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}Be concise.${role("user")}Hello${role("assistant")}Hi!${role("user")}How are you?${role("assistant")}Good, thanks!${role("user")}Goodbye`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
            "system": [
                {"type": "text", "text": "Be concise."}
            ],
            "messages": [
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

// ============================================================================
// Media passed through BAML function args
//
// Still on legacy Jinja prompt syntax: the BEP-049 structural assembler
// renders media interpolations as empty text (`prompt_value_text` in
// `sys_ops` handles scalars only; media is deferred per its M5d comment).
// ============================================================================

#[tokio::test]
#[ignore = "media values cannot be converted to VM values yet"]
async fn test_anthropic_mixed_text_and_image() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function F(img: image) -> string {
    client C
    prompt #"What is in this image? {{ img }}"#
}
function get_body(img: image) -> string {
    baml.llm.build_request(C, "F", { "img": img }).body
}
"##,
    ]
    .join("\n");
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
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "What is in this image?"},
                        {"type": "image", "source": {"type": "url", "url": "https://example.com/photo.jpg"}}
                    ]
                }
            ]
        })
    );
}

#[tokio::test]
#[ignore = "media values cannot be converted to VM values yet"]
async fn test_anthropic_audio_url() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function F(audio: audio) -> string {
    client C
    prompt #"Transcribe this audio: {{ audio }}"#
}
function get_body(audio: audio) -> string {
    baml.llm.build_request(C, "F", { "audio": audio }).body
}
"##,
    ]
    .join("\n");
    let audio = media_value(
        MediaKind::Audio,
        MediaContent::Url {
            url: "https://example.com/speech.mp3".into(),
            base64_data: None,
        },
        Some("audio/mpeg"),
    );
    let body = body_json(&run_baml_with_args(&source, "get_body", vec![audio]).await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Transcribe this audio:"},
                        {"type": "audio", "source": {"type": "url", "url": "https://example.com/speech.mp3"}}
                    ]
                }
            ]
        })
    );
}

// ============================================================================
// Anthropic — Multiple system messages combined
// ============================================================================

#[tokio::test]
#[ignore = "anthropic build_request missing max_tokens + merge_adjacent_roles concatenates content"]
async fn test_anthropic_multiple_system_messages() {
    let source = [
        ANTHROPIC_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("system")}You are concise.${role("user")}Hello`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 4096,
            "system": [
                {"type": "text", "text": "You are helpful."},
                {"type": "text", "text": "You are concise."}
            ],
            "messages": [
                {
                    "role": "user",
                    "content": [{"type": "text", "text": "Hello"}]
                }
            ]
        })
    );
}

// ============================================================================
// AWS Bedrock Integration Tests
// ============================================================================

/// Shared Bedrock client block.
const BEDROCK_CLIENT: &str = r#"
client C {
    provider aws-bedrock
    options {
        model "anthropic.claude-3-haiku-20240307-v1:0"
        region "us-west-2"
        access_key_id "AKIAIOSFODNN7EXAMPLE"
        secret_access_key "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
    }
}
"#;

// ============================================================================
// Prompt helper functions (template_string replacement)
// ============================================================================

#[tokio::test]
async fn test_bedrock_prompt_helper_expansion() {
    let source = [
        BEDROCK_CLIENT,
        r##"
function Greet(name: string) -> string {
    `Hello, ${name}!`
}
function F(name: string) -> string {
    client C
    prompt `${Greet(name)}`
}
function get_body() -> string {
    F$build_request("Alice").body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"text": "Hello, Alice!"}]}
            ]
        })
    );
}

// ============================================================================
// Struct args
// ============================================================================

#[tokio::test]
async fn test_bedrock_struct_arg_in_prompt() {
    let source = [
        BEDROCK_CLIENT,
        r##"
class Person {
    name string
    age int
}
function F(p: Person) -> string {
    client C
    prompt `${p.name} is ${p.age}`
}
function get_body() -> string {
    let p = Person { name: "Bob", age: 42 };
    F$build_request(p).body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"text": "Bob is 42"}]}
            ]
        })
    );
}

// ============================================================================
// System + user messages
// ============================================================================

#[tokio::test]
async fn test_bedrock_system_and_user() {
    let source = [
        BEDROCK_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "system": [{"text": "You are helpful."}],
            "messages": [
                {"role": "user", "content": [{"text": "Hi"}]}
            ]
        })
    );
}

// ============================================================================
// Multi-message conversations
// ============================================================================

#[tokio::test]
async fn test_bedrock_three_role_conversation() {
    let source = [
        BEDROCK_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are a helpful assistant.${role("user")}What is 2+2?${role("assistant")}4`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "system": [{"text": "You are a helpful assistant."}],
            "messages": [
                {"role": "user", "content": [{"text": "What is 2+2?"}]},
                {"role": "assistant", "content": [{"text": "4"}]}
            ]
        })
    );
}

#[tokio::test]
async fn test_bedrock_multi_turn_conversation() {
    let source = [
        BEDROCK_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}Be concise.${role("user")}Hello${role("assistant")}Hi!${role("user")}How are you?${role("assistant")}Good, thanks!${role("user")}Goodbye`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "system": [{"text": "Be concise."}],
            "messages": [
                {"role": "user", "content": [{"text": "Hello"}]},
                {"role": "assistant", "content": [{"text": "Hi!"}]},
                {"role": "user", "content": [{"text": "How are you?"}]},
                {"role": "assistant", "content": [{"text": "Good, thanks!"}]},
                {"role": "user", "content": [{"text": "Goodbye"}]}
            ]
        })
    );
}

// ============================================================================
// Inference config
// ============================================================================

#[tokio::test]
async fn test_bedrock_inference_config() {
    let source = r##"
client C {
    provider aws-bedrock
    options {
        model "anthropic.claude-3-haiku-20240307-v1:0"
        region "us-west-2"
        access_key_id "AKIAIOSFODNN7EXAMPLE"
        secret_access_key "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        max_tokens 500
        temperature 0.5
        top_p 0.75
    }
}
function F() -> string {
    client C
    prompt `${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"text": "Hi"}]}
            ],
            "inferenceConfig": {
                "maxTokens": 500,
                "temperature": 0.5,
                "topP": 0.75
            }
        })
    );
}

// ============================================================================
// User-only (no system message)
// ============================================================================

#[tokio::test]
async fn test_bedrock_user_only_no_system() {
    let source = [
        BEDROCK_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("user")}Hello there`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "messages": [
                {"role": "user", "content": [{"text": "Hello there"}]}
            ]
        })
    );
}

// ============================================================================
// Google AI / Vertex AI shared body tests
// ============================================================================

const GOOGLE_AI_CLIENT: &str = r#"
client C {
    provider google-ai
    options {
        model "gemini-2.0-flash"
        api_key "gemini-test-key"
    }
}
"#;

/// Uses API key auth via `query_params` to avoid ADC/OAuth token resolution in tests.
const VERTEX_AI_CLIENT: &str = r#"
client C {
    provider vertex-ai
    options {
        model "gemini-2.0-flash"
        base_url "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/publishers/google/models"
        query_params { key "test-api-key" }
    }
}
"#;

macro_rules! gemini_body_tests {
    ($mod:ident, $client:expr) => {
        mod $mod {
            use super::*;

            #[tokio::test]
            async fn prompt_helper_expansion() {
                let source = [
                    $client,
                    r##"
function Greet(name: string) -> string {
    `Hello, ${name}!`
}
function F(name: string) -> string {
    client C
    prompt `${Greet(name)}`
}
function get_body() -> string {
    F$build_request("Alice").body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Hello, Alice!"}]}
                        ]
                    })
                );
            }

            #[tokio::test]
            async fn struct_arg_in_prompt() {
                let source = [
                    $client,
                    r##"
class Person {
    name string
    age int
}
function F(p: Person) -> string {
    client C
    prompt `${p.name} is ${p.age}`
}
function get_body() -> string {
    let p = Person { name: "Bob", age: 42 };
    F$build_request(p).body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Bob is 42"}]}
                        ]
                    })
                );
            }

            #[tokio::test]
            async fn system_and_user() {
                let source = [
                    $client,
                    r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Hi"}]}
                        ],
                        "systemInstruction": {
                            "parts": [{"text": "You are helpful."}]
                        }
                    })
                );
            }

            #[tokio::test]
            async fn three_role_conversation() {
                let source = [
                    $client,
                    r##"
function F() -> string {
    client C
    prompt `${role("system")}You are a helpful assistant.${role("user")}What is 2+2?${role("assistant")}4`
}
function get_body() -> string {
    F$build_request().body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "What is 2+2?"}]},
                            {"role": "model", "parts": [{"text": "4"}]}
                        ],
                        "systemInstruction": {
                            "parts": [{"text": "You are a helpful assistant."}]
                        }
                    })
                );
            }

            #[tokio::test]
            async fn multi_turn_conversation() {
                let source = [
                    $client,
                    r##"
function F() -> string {
    client C
    prompt `${role("system")}Be concise.${role("user")}Hello${role("assistant")}Hi!${role("user")}How are you?${role("assistant")}Good, thanks!${role("user")}Goodbye`
}
function get_body() -> string {
    F$build_request().body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Hello"}]},
                            {"role": "model", "parts": [{"text": "Hi!"}]},
                            {"role": "user", "parts": [{"text": "How are you?"}]},
                            {"role": "model", "parts": [{"text": "Good, thanks!"}]},
                            {"role": "user", "parts": [{"text": "Goodbye"}]}
                        ],
                        "systemInstruction": {
                            "parts": [{"text": "Be concise."}]
                        }
                    })
                );
            }

            #[tokio::test]
            async fn user_only_no_system() {
                let source = [
                    $client,
                    r##"
function F() -> string {
    client C
    prompt `${role("user")}Hello there`
}
function get_body() -> string {
    F$build_request().body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Hello there"}]}
                        ]
                    })
                );
            }

            #[tokio::test]
            async fn assistant_remapped_to_model() {
                let source = [
                    $client,
                    r##"
function F() -> string {
    client C
    prompt `${role("user")}Hi${role("assistant")}Hello!`
}
function get_body() -> string {
    F$build_request().body
}
"##,
                ]
                .join("\n");

                let body = body_json(&run_baml(&source, "get_body").await);
                assert_eq!(
                    body,
                    serde_json::json!({
                        "contents": [
                            {"role": "user", "parts": [{"text": "Hi"}]},
                            {"role": "model", "parts": [{"text": "Hello!"}]}
                        ]
                    })
                );
            }
        }
    };
}

gemini_body_tests!(google_ai, GOOGLE_AI_CLIENT);
gemini_body_tests!(vertex_ai, VERTEX_AI_CLIENT);

// ============================================================================
// Vertex AI + Anthropic (rawPredict) e2e tests
// ============================================================================

const VERTEX_CLAUDE_CLIENT: &str = r#"
client C {
    provider vertex-ai
    options {
        model "claude-sonnet-4-20250514"
        base_url "https://us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/publishers/anthropic/models"
        query_params { key "test-api-key" }
    }
}
"#;

#[tokio::test]
async fn vertex_claude_uses_raw_predict_url() {
    let source = [
        VERTEX_CLAUDE_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("user")}Hi`
}
function get_url() -> string {
    F$build_request().url
}
"##,
    ]
    .join("\n");

    let result = run_baml(&source, "get_url").await;
    let url = as_string(&result);
    assert!(
        url.contains(":rawPredict"),
        "Claude on Vertex should use rawPredict: {url}"
    );
    assert!(
        !url.contains(":generateContent"),
        "Claude on Vertex should NOT use generateContent: {url}"
    );
}

#[tokio::test]
async fn vertex_claude_keeps_assistant_role() {
    let source = [
        VERTEX_CLAUDE_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("user")}Hi${role("assistant")}Hello!${role("user")}How are you?`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 8192,
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]},
                {"role": "user", "content": [{"type": "text", "text": "How are you?"}]}
            ]
        })
    );
}

#[tokio::test]
async fn vertex_claude_system_extracted() {
    let source = [
        VERTEX_CLAUDE_CLIENT,
        r##"
function F() -> string {
    client C
    prompt `${role("system")}You are helpful.${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##,
    ]
    .join("\n");

    let body = body_json(&run_baml(&source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "model": "claude-sonnet-4-20250514",
            "anthropic_version": "vertex-2023-10-16",
            "max_tokens": 8192,
            "system": [{"type": "text", "text": "You are helpful."}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "Hi"}]}
            ]
        })
    );
}

#[tokio::test]
async fn test_google_ai_generation_config() {
    let source = r##"
client C {
    provider google-ai
    options {
        model "gemini-2.0-flash"
        api_key "gemini-test-key"
        generationConfig {
            temperature 0.7
            maxOutputTokens 1024
        }
    }
}
function F() -> string {
    client C
    prompt `${role("user")}Hi`
}
function get_body() -> string {
    F$build_request().body
}
"##;

    let body = body_json(&run_baml(source, "get_body").await);
    assert_eq!(
        body,
        serde_json::json!({
            "contents": [
                {"role": "user", "parts": [{"text": "Hi"}]}
            ],
            "generationConfig": {
                "temperature": 0.7,
                "maxOutputTokens": 1024
            }
        })
    );
}
