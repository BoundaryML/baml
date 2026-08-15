//! Provider request builders consume the structural `ai.Prompt` produced by an
//! LLM spec. These tests stay offline and inspect only the serialized body.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

async fn request_body(expr: &str) -> serde_json::Value {
    let source = format!(
        r#"
function RequestShape() -> string {{
  client: "openai/gpt-4o-mini"
  prompt: `${{role("system")}}Follow the rules.${{role("user")}}Answer this.${{ctx.output_format}}`
}}

function main() -> string {{
  let spec = RequestShape$spec()
  let input = ai.ModelTurnInput {{
    prompt: spec.prompt_template,
    journal: ai.Journal {{ log: [] }},
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
  }}
  {expr}.body
}}
"#
    );
    let output = baml_test!(&source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => body.to_string(),
        other => panic!("expected a request body string, got {other:?}"),
    };
    serde_json::from_str(&body).expect("provider request body should be valid JSON")
}

#[tokio::test]
async fn openai_preserves_prompt_message_roles() {
    let body = request_body(
        r#"openai.internal.openai_render(
    openai.OpenAiClient.new(model = "gpt-test", api_key = "test-key"),
    input,
  )"#,
    )
    .await;
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["role"], "system");
    assert_eq!(input[0]["content"], "Follow the rules.");
    assert_eq!(input[1]["role"], "user");
    assert!(
        input[1]["content"]
            .as_str()
            .expect("user prompt text")
            .starts_with("Answer this.")
    );
    assert!(body.get("instructions").is_none());
}

#[tokio::test]
async fn anthropic_splits_system_from_prompt_messages() {
    let body = request_body(
        r#"anthropic.internal._anthropic_request(
    anthropic.AnthropicClient.new(model = "claude-test", api_key = "test-key"),
    input,
    false,
  )"#,
    )
    .await;
    assert_eq!(body["system"][0]["type"], "text");
    assert_eq!(body["system"][0]["text"], "Follow the rules.");
    assert_eq!(body["messages"][0]["role"], "user");
    assert!(
        body["messages"][0]["content"][0]["text"]
            .as_str()
            .expect("Anthropic user text")
            .starts_with("Answer this.")
    );
}

#[tokio::test]
async fn google_splits_system_and_maps_user_prompt() {
    let body = request_body(
        r#"google.internal.google_render(
    google.GoogleClient.new(model = "gemini-test", api_key = "test-key"),
    input,
  )"#,
    )
    .await;
    assert_eq!(
        body["systemInstruction"]["parts"][0]["text"],
        "Follow the rules."
    );
    assert_eq!(body["contents"][0]["role"], "user");
    assert!(
        body["contents"][0]["parts"][0]["text"]
            .as_str()
            .expect("Gemini user text")
            .starts_with("Answer this.")
    );
}
