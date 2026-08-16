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
async fn generated_build_request_companion_is_network_free_and_overridable() {
    let source = r#"
client Override = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function RequestShape(input: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `${input} ${ctx.output_format}`
}

function main() -> string {
  RequestShape$build_request("hello", client = Override).url
}
"#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "http://localhost:1234/v1/responses".to_string().into()
        ))
    );
}

#[tokio::test]
async fn generated_companions_apply_defaulted_llm_arguments() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Greet(name: string, suffix: string = "!") -> string {
  client: Preview
  prompt: `Hello ${name}${suffix}`
}

function main() -> string {
  Greet$render_prompt("Ada").text() + "|" + Greet$build_request("Ada").body
}
"#;
    let output = baml_test!(source);
    let result = match output.result {
        Ok(BexExternalValue::String(result)) => result.to_string(),
        other => panic!("expected defaulted companion output, got {other:?}"),
    };
    assert!(
        result.starts_with("[system]\nHello Ada!|"),
        "rendered prompt: {result}"
    );
    assert!(result.contains("Hello Ada!"), "request body: {result}");
}

#[tokio::test]
async fn generated_build_request_companion_is_reflection_visible() {
    let source = r#"
client Override = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function RequestShape(input: string) -> string {
  client: Override
  prompt: `${input} ${ctx.output_format}`
}

function main() -> string {
  let f: baml.AnyFunction<Returns = baml.http.Request, Throws = ai.errors.Failure | baml.errors.InvalidArgument | baml.errors.Io | baml.errors.ParseError | baml.errors.UnknownError | baml.reflect.errors.CompilationError> = RequestShape$build_request
  let request = reflect.call_any(f, { "input": "hello", "client": Override })
  request.url
}
"#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "http://localhost:1234/v1/responses".to_string().into()
        ))
    );
}

#[tokio::test]
async fn generic_nested_output_format_uses_concrete_class_arguments() {
    let source = r#"
enum Choice {
  Empty
}

class Inner<T> {
  values: T[]
}

class Outer<T> {
  inner: Inner<T>
}

function main() -> string {
  ai.wire.render_output_format(type.of<Outer<Choice>>())
}
"#;
    let output = baml_test!(source);
    let rendered = match output.result {
        Ok(BexExternalValue::String(rendered)) => rendered.to_string(),
        other => panic!("expected a nested generic output format, got {other:?}"),
    };
    assert!(
        rendered.contains("'Empty'"),
        "rendered schema should contain the concrete Choice variant: {rendered}"
    );
    assert!(!rendered.contains("TypeVar"), "rendered schema: {rendered}");
    assert!(!rendered.contains("T[]"), "rendered schema: {rendered}");
}

#[tokio::test]
async fn generic_build_request_specializes_nested_output_format() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
  model = "gpt-test",
  api_key = "test-key",
  base_url = "http://localhost:1234/v1",
)

enum Choice {
  Empty
}

class Inner<T> {
  values: T[]
}

class Outer<T> {
  inner: Inner<T>
}

function GenericShape<T>() -> Outer<T> {
  client: Preview
  prompt: `${ctx.output_format}`
}

function main() -> string {
  GenericShape$build_request<Choice>().body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => body.to_string(),
        other => panic!("expected a specialized generic request body, got {other:?}"),
    };
    assert!(
        body.contains("'Empty'"),
        "request schema should contain the concrete Choice variant: {body}"
    );
    assert!(!body.contains("TypeVar"), "request body: {body}");
    assert!(!body.contains("T[]"), "request body: {body}");
}

#[tokio::test]
async fn client_wrapper_rendering_is_pure_and_deterministic() {
    let source = r#"
client First = openai.ResponsesClient.new(
    model = "first",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);
client Second = openai.ResponsesClient.new(
    model = "second",
    api_key = "test-key",
    base_url = "http://localhost:5678/v1",
);

function RequestShape() -> string {
  client: First
  prompt: `hello ${ctx.output_format}`
}

function main() -> string {
  let input = ai.ModelTurnInput {
    prompt: RequestShape$spec().prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
  }
  let retry = ai.clients.Retry.new(First)
  let fallback = ai.clients.Fallback { members: [First, Second] }
  let round_robin = ai.clients.RoundRobin.new([First, Second])
  retry.render(input).url + "|" + fallback.render(input).url + "|" + round_robin.render(input).url
}
"#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "http://localhost:1234/v1/responses|http://localhost:1234/v1/responses|http://localhost:1234/v1/responses".to_string().into()
        ))
    );
}

#[tokio::test]
async fn client_render_smoke_covers_non_responses_providers() {
    let source = r#"
client Anthropic = anthropic.AnthropicClient.new(
  model = "claude-test",
  api_key = "test-key",
  base_url = "http://localhost:1234",
)
client Gemini = google.GoogleClient.new(
  model = "gemini-test",
  api_key = "test-key",
  base_url = "http://localhost:1234/v1",
)
client Bedrock = aws.BedrockClient.new(
  model = "amazon.test",
  endpoint_url = "http://localhost:1234",
)
client Chat = openai.ChatClient.new(
  model = "chat-test",
  api_key = "test-key",
  base_url = "http://localhost:1234/v1",
)
client Images = openai.ImageClient.new(
  model = "gpt-image-1",
  api_key = "test-key",
  base_url = "http://localhost:1234/v1",
)

function Shape() -> string {
  client: Anthropic
  prompt: `hello ${ctx.output_format}`
}

function main() -> string {
  let input = ai.ModelTurnInput {
    prompt: Shape$spec().prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
  }
  let a: ai.Client = Anthropic
  let g: ai.Client = Gemini
  let b: ai.Client = Bedrock
  let c: ai.Client = Chat
  let i: ai.Client = Images
  a.render(input).url + "|" + g.render(input).url + "|" + b.render(input).url + "|" + c.render(input).url + "|" + i.render(input).url
}
"#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "http://localhost:1234/v1/messages|http://localhost:1234/v1/models/gemini-test:generateContent|http://localhost:1234/model/amazon.test/converse|http://localhost:1234/v1/chat/completions|http://localhost:1234/v1/images/generations".to_string().into()
        ))
    );
}

#[tokio::test]
async fn preview_media_never_reads_files_or_fetches_urls() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://127.0.0.1:9/v1",
);

function MediaShape(photo: image) -> string {
  client: Preview
  prompt: `${role("user")}Inspect:${photo}`
}

function main() -> string {
  let file_result = MediaShape$build_request(
    image.from_file("/definitely/not-a-real-preview-file.png", "image/png"),
    client = Preview,
  ).body catch_all (e) {
    ai.errors.PreviewUnsupported => "preview",
    _ => "wrong:" + e.to_string(),
  }
  let url_result = MediaShape$build_request(
    image.from_url("http://127.0.0.1:9/image.png", "image/png"),
    client = Preview,
  ).body catch_all (e) {
    _ => "wrong:" + e.to_string(),
  }
  file_result + "|" + url_result
}
"#;
    let output = baml_test!(source);
    let result = match output.result {
        Ok(BexExternalValue::String(result)) => result.to_string(),
        other => panic!("expected preview results, got {other:?}"),
    };
    assert!(
        result.starts_with("preview|"),
        "file media should fail with the typed preview error without opening the path: {result}"
    );
    assert!(
        result.contains("http://127.0.0.1:9/image.png"),
        "URL media should remain a reference without making a network request: {result}"
    );
    assert!(
        !result.contains("wrong:"),
        "preview media should not surface transport or filesystem errors: {result}"
    );
}

#[tokio::test]
async fn chat_preview_preserves_urls_unless_invoke_would_inline_them() {
    let source = r#"
client Chat = openai.ChatClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://127.0.0.1:9/v1",
);
client Ollama = openai.OllamaClient.new(
    model = "local-test",
    base_url = "http://127.0.0.1:9/v1",
);

function MediaShape(photo: image) -> string {
  client: Chat
  prompt: `${role("user")}Inspect:${photo}`
}

function main() -> string {
  let photo = image.from_url("http://127.0.0.1:9/image.png", "image/png")
  let chat = MediaShape$build_request(photo, client = Chat).body
  let ollama = MediaShape$build_request(photo, client = Ollama).body catch_all (e) {
    ai.errors.PreviewUnsupported => "unsupported",
    _ => "wrong:" + e.to_string(),
  }
  chat + "|" + ollama
}
"#;
    let output = baml_test!(source);
    let result = match output.result {
        Ok(BexExternalValue::String(result)) => result.to_string(),
        other => panic!("expected chat preview results, got {other:?}"),
    };
    assert!(
        result.contains("http://127.0.0.1:9/image.png"),
        "OpenAI-compatible chat preview should preserve its supported URL wire form: {result}"
    );
    assert!(
        result.ends_with("|unsupported"),
        "Ollama preview should reject the URL that invoke would have to download: {result}"
    );
    assert!(
        !result.contains("wrong:"),
        "unexpected preview error: {result}"
    );
}

#[tokio::test]
async fn vertex_project_id_accepts_late_bound_env_ref_in_preview() {
    const CHILD_MARKER: &str = "BAML_VERTEX_PROJECT_PREVIEW_CHILD";
    if std::env::var(CHILD_MARKER).as_deref() != Ok("1") {
        let status = std::process::Command::new(
            std::env::current_exe().expect("test binary path should be available"),
        )
        .arg("--exact")
        .arg("vertex_project_id_accepts_late_bound_env_ref_in_preview")
        .arg("--nocapture")
        .env(CHILD_MARKER, "1")
        .env("VERTEX_PROJECT_ID", "test-project")
        .status()
        .expect("isolated preview test process should start");
        assert!(status.success(), "isolated preview test failed: {status}");
        return;
    }

    let source = r#"
client Vertex = google.VertexClient.new(
  model = "gemini-test",
  project_id = env.VERTEX_PROJECT_ID,
  location = "us-central1",
  api_key = "test-key",
  headers = { "x-preview": "yes" },
  query_params = { "trace": "enabled" },
  request_body = baml.json.parse(`{"preview_marker":"kept"}`),
)

function Shape() -> string {
  client: Vertex
  prompt: `hello`
}

function main() -> string {
  let expected = env.VERTEX_PROJECT_ID.get_or_panic()
  let input = ai.ModelTurnInput {
    prompt: Shape$spec().prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
  }
  let c: ai.Client = Vertex
  let request = c.render(input)
  let body = baml.json.parse(request.body)
  let base_client = google.VertexClient.new(
    model = "gemini-override",
    base_url = "https://preview.example/v1/models",
    api_key = "override-key",
    headers = { "x-base": "yes" },
    query_params = { "q": "one" },
    request_body = baml.json.parse(`{"base_marker":"kept"}`),
  )
  let base_request: ai.Client = base_client
  let override = base_request.render(input)
  let override_body = baml.json.parse(override.body)
  if (
    request.url.includes(expected)
    && request.url.includes("/locations/us-central1/")
    && request.url.includes("trace=enabled")
    && request.headers.get("x-preview") == "yes"
    && baml.json.path<string>(body, ".preview_marker") == "kept"
    && override.url.includes("https://preview.example/v1/models/gemini-override")
    && override.url.includes("q=one")
    && override.headers.get("x-base") == "yes"
    && baml.json.path<string>(override_body, ".base_marker") == "kept"
  ) { "ok" } else { "wrong" }
}
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string().into()))
    );
}

async fn media_request_body(
    expr: &str,
    include_video: bool,
    include_audio: bool,
) -> serde_json::Value {
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
  client: "google/gemini-2.5-flash"
  tools: []
  prompt: `${{role("user")}}Inspect:${{photo}}:{audio_prompt}${{document}}:{video_prompt}`
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
    output_type: type.of<string>(),
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
async fn openai_build_request_defaults_roleless_llm_prompt_to_system() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Shape() -> string {
  client: Preview
  prompt: `Answer briefly.`
}

function main() -> string {
  Shape$build_request().body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => {
            serde_json::from_str::<serde_json::Value>(body.as_str()).unwrap()
        }
        other => panic!("expected an OpenAI request body, got {other:?}"),
    };
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["role"], "system");
    assert_eq!(input[0]["content"][0]["text"], "Answer briefly.");
}

#[tokio::test]
async fn openai_build_request_allows_image_in_roleless_system_prompt() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Shape(photo: image) -> string {
  client: Preview
  prompt: `Inspect the image: ${photo}`
}

function main() -> string {
  Shape$build_request(image.from_base64("image-data", "image/png")).body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => {
            serde_json::from_str::<serde_json::Value>(body.as_str()).unwrap()
        }
        other => panic!("expected an OpenAI request body, got {other:?}"),
    };
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["role"], "system");
    assert_eq!(input[0]["content"][0]["type"], "input_text");
    assert_eq!(input[0]["content"][1]["type"], "input_image");
    assert_eq!(
        input[0]["content"][1]["image_url"],
        "data:image/png;base64,image-data"
    );
}

#[tokio::test]
async fn openai_build_request_preserves_explicit_llm_prompt_roles() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Shape() -> string {
  client: Preview
  prompt: `${role("user")}Answer this.${role("system")}Follow the rules.`
}

function main() -> string {
  Shape$build_request().body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => {
            serde_json::from_str::<serde_json::Value>(body.as_str()).unwrap()
        }
        other => panic!("expected an OpenAI request body, got {other:?}"),
    };
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["role"], "user");
    assert_eq!(input[0]["content"][0]["text"], "Answer this.");
    assert_eq!(input[1]["role"], "system");
    assert_eq!(input[1]["content"][0]["text"], "Follow the rules.");
}

#[tokio::test]
async fn openai_build_request_defaults_leading_llm_prelude_to_system() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Shape() -> string {
  client: Preview
  prompt: `You are a differential assistant.\n${role("user")}Answer this.`
}

function main() -> string {
  Shape$build_request().body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => {
            serde_json::from_str::<serde_json::Value>(body.as_str()).unwrap()
        }
        other => panic!("expected an OpenAI request body, got {other:?}"),
    };
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 2);
    assert_eq!(input[0]["role"], "system");
    assert_eq!(
        input[0]["content"][0]["text"],
        "You are a differential assistant.\n"
    );
    assert_eq!(input[1]["role"], "user");
    assert_eq!(input[1]["content"][0]["text"], "Answer this.");
}

#[tokio::test]
async fn openai_build_request_ignores_formatting_whitespace_before_first_role() {
    let source = r#"
client Preview = openai.ResponsesClient.new(
    model = "gpt-test",
    api_key = "test-key",
    base_url = "http://localhost:1234/v1",
);

function Shape() -> string {
  client: Preview
  prompt: `
    ${role("user")}Answer this.
  `
}

function main() -> string {
  Shape$build_request().body
}
"#;
    let output = baml_test!(source);
    let body = match output.result {
        Ok(BexExternalValue::String(body)) => {
            serde_json::from_str::<serde_json::Value>(body.as_str()).unwrap()
        }
        other => panic!("expected an OpenAI request body, got {other:?}"),
    };
    let input = body["input"].as_array().expect("OpenAI input array");
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["role"], "user");
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
  client: "google/gemini-2.5-flash"
  tools: []
  prompt: `${role("user")}Listen:${sound}`
}

function main() -> string {
  let spec = AudioShape$spec(audio.from_base64("audio-data", "audio/mpeg"))
  let input = ai.ModelTurnInput {
    prompt: spec.prompt_template,
    journal: ai.Journal { log: [] },
    toolbox: ai.tools.Toolbox.new([]),
    output_type: type.of<string>(),
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
