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
    output_type: reflect.type_of<string>(),
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

async fn media_request_body(expr: &str, include_video: bool, include_audio: bool) -> serde_json::Value {
    let video_parameter = if include_video { ", movie: video" } else { "" };
    let video_prompt = if include_video { "${movie}" } else { "" };
    let video_argument = if include_video {
        r#", video.from_base64("video-data", "video/mp4")"#
    } else {
        ""
    };
    // Anthropic's Messages API has no audio input block; its lowering rejects
    // audio parts, so that provider's shape test opts out of the audio param.
    let audio_parameter = if include_audio { ", sound: audio" } else { "" };
    let audio_prompt = if include_audio { "${sound}:" } else { "" };
    let audio_argument = if include_audio {
        r#"audio.from_base64("audio-data", "audio/mpeg"),"#
    } else {
        ""
    };
    let source = format!(
        r#"
function MediaShape(photo: image{audio_parameter}, document: pdf{video_parameter}) -> string {{
  client "google/gemini-2.5-flash"
  tools []
  prompt `${{role("user")}}Inspect:${{photo}}:{audio_prompt}${{document}}:{video_prompt}`
}}

function main() -> string {{
  let spec = MediaShape$spec(
    image.from_base64("image-data", "image/png"),
    {audio_argument}
    pdf.from_base64("pdf-data", "application/pdf")
    {video_argument}
  )
  let input = ai.ModelTurnInput {{
    prompt: spec.prompt_template,
    journal: ai.Journal {{ log: [] }},
    toolbox: ai.tools.Toolbox.new([]),
    output_type: reflect.type_of<string>(),
  }}
  {expr}.body
}}
"#
    );
    let output = baml_test!(&source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => body.to_string(),
        other => panic!("expected a media request body string, got {other:?}"),
    };
    serde_json::from_str(&body).expect("provider media request body should be valid JSON")
}

#[tokio::test]
async fn openai_preserves_prompt_message_roles() {
    let body = request_body(
        r#"openai.internal.openai_render(
    openai.ResponsesClient.new(model = "gpt-test", api_key = "test-key"),
    input,
  )"#,
    )
    .await;
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["role"], "system");
    assert_eq!(input[0]["content"][0]["type"], "input_text");
    assert_eq!(input[0]["content"][0]["text"], "Follow the rules.");
    assert_eq!(input[1]["role"], "user");
    assert!(
        input[1]["content"][0]["text"]
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

#[tokio::test]
async fn openai_lowers_image_audio_and_pdf_parts() {
    let body = media_request_body(
        r#"openai.internal.openai_render(
    openai.ResponsesClient.new(model = "gpt-test", api_key = "test-key"),
    input,
  )"#,
        false,
        true,
    )
    .await;
    let parts = body["input"][0]["content"]
        .as_array()
        .expect("OpenAI content parts");
    let by_type = |kind: &str| {
        parts
            .iter()
            .find(|part| part["type"] == kind)
            .unwrap_or_else(|| panic!("missing OpenAI {kind} part: {parts:?}"))
    };
    assert_eq!(
        by_type("input_image")["image_url"],
        "data:image/png;base64,image-data"
    );
    assert_eq!(by_type("input_audio")["input_audio"]["data"], "audio-data");
    assert_eq!(by_type("input_audio")["input_audio"]["format"], "mp3");
    assert_eq!(
        by_type("input_file")["file_data"],
        "data:application/pdf;base64,pdf-data"
    );
}

#[tokio::test]
async fn anthropic_lowers_image_and_pdf_parts() {
    let body = media_request_body(
        r#"anthropic.internal._anthropic_request(
    anthropic.AnthropicClient.new(model = "claude-test", api_key = "test-key"),
    input,
    false,
  )"#,
        false,
        false,
    )
    .await;
    let parts = body["messages"][0]["content"]
        .as_array()
        .expect("Anthropic content parts");
    let by_type = |kind: &str| {
        parts
            .iter()
            .find(|part| part["type"] == kind)
            .unwrap_or_else(|| panic!("missing Anthropic {kind} part: {parts:?}"))
    };
    assert_eq!(by_type("image")["source"]["type"], "base64");
    assert_eq!(by_type("image")["source"]["data"], "image-data");
    assert_eq!(by_type("document")["source"]["data"], "pdf-data");
}

// The Messages API has no audio input content block; the lowering rejects it
// with a typed error instead of inventing a wire shape.
#[tokio::test]
async fn anthropic_rejects_audio_parts() {
    let source = r#"
function AudioShape(sound: audio) -> string {
  client "google/gemini-2.5-flash"
  tools []
  prompt `${role("user")}Listen:${sound}`
}

function main() -> string {
  let spec = AudioShape$spec(audio.from_base64("audio-data", "audio/mpeg"))
  let input = ai.ModelTurnInput {
    prompt: spec.prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: reflect.type_of<string>(),
  }
  anthropic.internal._anthropic_request(
    anthropic.AnthropicClient.new(model = "claude-test", api_key = "test-key"),
    input,
    false,
  ).body catch_all (e) {
    _ => e.to_string(),
  }
}
"#;
    let output = baml_test!(source);
    let message = match output.result {
        Ok(BexExternalValue::String(message)) => message.to_string(),
        other => panic!("expected the rejection message, got {other:?}"),
    };
    assert!(
        message.contains("audio"),
        "expected an audio rejection, got: {message}"
    );
}

#[tokio::test]
async fn google_lowers_every_supported_media_part() {
    let body = media_request_body(
        r#"google.internal.google_render(
    google.GoogleClient.new(model = "gemini-test", api_key = "test-key"),
    input,
  )"#,
        true,
        true,
    )
    .await;
    let parts = body["contents"][0]["parts"]
        .as_array()
        .expect("Gemini content parts");
    let inline_parts: Vec<_> = parts
        .iter()
        .filter_map(|part| part.get("inlineData"))
        .collect();
    assert_eq!(inline_parts.len(), 4, "Gemini media parts: {parts:?}");
    assert_eq!(inline_parts[0]["mimeType"], "image/png");
    assert_eq!(inline_parts[0]["data"], "image-data");
    assert_eq!(inline_parts[1]["mimeType"], "audio/mpeg");
    assert_eq!(inline_parts[1]["data"], "audio-data");
    assert_eq!(inline_parts[2]["mimeType"], "application/pdf");
    assert_eq!(inline_parts[2]["data"], "pdf-data");
    assert_eq!(inline_parts[3]["mimeType"], "video/mp4");
    assert_eq!(inline_parts[3]["data"], "video-data");
}
