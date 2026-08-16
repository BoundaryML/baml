//! Offline request-preview coverage for provider credentials and config.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn result_string(output: baml_tests::engine::TestOutput) -> String {
    match output.result {
        Ok(BexExternalValue::String(value)) => value.to_string(),
        other => panic!("expected a string result, got {other:?}"),
    }
}

#[tokio::test]
async fn provider_previews_are_keyless_and_preserve_user_headers() {
    let output = baml_test!(
        r#"
function PreviewShape() -> string {
  client: "openai/gpt-4o-mini"
  prompt: `hello`
}

function preview_input() -> ai.ModelTurnInput {
  let spec = PreviewShape@spec()
  ai.ModelTurnInput {
    prompt: spec.prompt_template,
    journal: ai.Journal.new(spec),
    toolbox: spec.tools(),
    output_type: spec.output_type(),
  }
}

function main() -> string {
  let responses: ai.Client = openai.ResponsesClient.new(
    model = "gpt-preview",
    api_key = env.BAML_PREVIEW_OPENAI_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "responses" },
  )
  let anthropic_client: ai.Client = anthropic.AnthropicClient.new(
    model = "claude-preview",
    api_key = env.BAML_PREVIEW_ANTHROPIC_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "anthropic" },
  )
  let gateway: ai.Client = vercel.AiGatewayImageClient.new(
    model = "bfl/flux-preview",
    api_key = env.BAML_PREVIEW_GATEWAY_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "gateway" },
  )
  let chat: ai.Client = openai.ChatClient.new(
    model = "gpt-preview",
    api_key = env.BAML_PREVIEW_CHAT_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "chat" },
  )
  let image: ai.Client = openai.ImageClient.new(
    model = "gpt-image-preview",
    api_key = env.BAML_PREVIEW_IMAGE_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "image" },
  )

  let responses_request = responses.render(preview_input())
  let anthropic_request = anthropic_client.render(preview_input())
  let gateway_request = gateway.render(preview_input())
  let chat_request = chat.render(preview_input())
  let image_request = image.render(preview_input())

  let responses_configured: ai.Client = openai.ResponsesClient.new(
    model = "gpt-preview",
    api_key = "responses-secret",
    headers = { "Authorization": "user-responses", "x-config": "kept" },
  )
  let anthropic_configured: ai.Client = anthropic.AnthropicClient.new(
    model = "claude-preview",
    api_key = "anthropic-secret",
    headers = { "X-Api-Key": "user-anthropic", "x-config": "kept" },
  )
  let gateway_configured: ai.Client = vercel.AiGatewayImageClient.new(
    model = "bfl/flux-preview",
    api_key = "gateway-secret",
    headers = {
      "Authorization": "user-gateway",
      "Ai-Gateway-Auth-Method": "user-method",
      "x-config": "kept",
    },
  )
  let chat_configured: ai.Client = openai.ChatClient.new(
    model = "gpt-preview",
    api_key = "chat-secret",
    headers = { "Authorization": "user-chat", "x-config": "kept" },
  )
  let image_automatic: ai.Client = openai.ImageClient.new(
    model = "gpt-image-preview",
    api_key = "image-secret",
    headers = { "x-config": "automatic" },
  )
  let image_configured: ai.Client = openai.ImageClient.new(
    model = "gpt-image-preview",
    api_key = "image-secret",
    headers = { "Authorization": "user-image", "x-config": "kept" },
  )

  let responses_user = responses_configured.render(preview_input())
  let anthropic_user = anthropic_configured.render(preview_input())
  let gateway_user = gateway_configured.render(preview_input())
  let chat_user = chat_configured.render(preview_input())
  let image_automatic_request = image_automatic.render(preview_input())
  let image_user = image_configured.render(preview_input())

  `responses=${responses_request.headers.get("authorization") ?? "none"}:${responses_request.headers.get("x-user") ?? "none"}`
    + `|anthropic=${anthropic_request.headers.get("x-api-key") ?? "none"}:${anthropic_request.headers.get("x-user") ?? "none"}`
    + `|gateway=${gateway_request.headers.get("authorization") ?? "none"}:${gateway_request.headers.get("ai-gateway-auth-method") ?? "none"}:${gateway_request.headers.get("x-user") ?? "none"}`
    + `|chat=${chat_request.headers.get("authorization") ?? "none"}:${chat_request.headers.get("x-user") ?? "none"}`
    + `|image=${image_request.headers.get("authorization") ?? "none"}:${image_request.headers.get("x-user") ?? "none"}`
    + `|responses_user=${responses_user.headers.get("authorization") ?? "none"}:${responses_user.headers.get("x-config") ?? "none"}`
    + `|anthropic_user=${anthropic_user.headers.get("x-api-key") ?? "none"}:${anthropic_user.headers.get("x-config") ?? "none"}`
    + `|gateway_user=${gateway_user.headers.get("authorization") ?? "none"}:${gateway_user.headers.get("ai-gateway-auth-method") ?? "none"}:${gateway_user.headers.get("x-config") ?? "none"}`
    + `|chat_user=${chat_user.headers.get("authorization") ?? "none"}:${chat_user.headers.get("x-config") ?? "none"}`
    + `|image_automatic=${image_automatic_request.headers.get("authorization") ?? "none"}:${image_automatic_request.headers.get("x-config") ?? "none"}`
    + `|image_user=${image_user.headers.get("authorization") ?? "none"}:${image_user.headers.get("x-config") ?? "none"}`
}
"#
    );

    assert_eq!(
        result_string(output),
        "responses=none:responses|anthropic=none:anthropic|gateway=none:none:gateway|chat=none:chat|image=none:image|responses_user=user-responses:kept|anthropic_user=user-anthropic:kept|gateway_user=user-gateway:user-method:kept|chat_user=user-chat:kept|image_automatic=none:automatic|image_user=user-image:kept"
    );
}

#[tokio::test]
async fn google_previews_use_deterministic_secret_free_markers() {
    let output = baml_test!(
        r#"
function PreviewShape() -> string {
  client: "openai/gpt-4o-mini"
  prompt: `hello`
}

function preview_input() -> ai.ModelTurnInput {
  let spec = PreviewShape@spec()
  ai.ModelTurnInput {
    prompt: spec.prompt_template,
    journal: ai.Journal.new(spec),
    toolbox: spec.tools(),
    output_type: spec.output_type(),
  }
}

function main() -> string {
  let gemini: ai.Client = google.GoogleClient.new(
    model = "gemini-preview",
    api_key = env.BAML_PREVIEW_GOOGLE_KEY_THAT_IS_NEVER_SET,
    headers = { "x-user": "gemini" },
    query_params = { "trace": "kept" },
    request_body = baml.json.parse(`{"preview_config":"kept"}`),
  )
  let gemini_request = gemini.render(preview_input())

  let gemini_configured: ai.Client = google.GoogleClient.new(
    model = "gemini-preview",
    api_key = "gemini-config-secret",
    headers = { "X-Goog-Api-Key": "user-gemini-key" },
  )
  let gemini_user_request = gemini_configured.render(preview_input())

  let vertex: ai.Client = google.VertexClient.new(
    model = "gemini-preview",
    api_key = "vertex-secret",
    headers = { "x-user": "vertex" },
    query_params = { "trace": "kept" },
    request_body = baml.json.parse(`{"preview_config":"kept"}`),
  )
  let vertex_request = vertex.render(preview_input())

  let vertex_ref: ai.Client = google.VertexClient.new(
    model = "gemini-preview",
    api_key = env.BAML_PREVIEW_VERTEX_KEY_THAT_IS_NEVER_SET,
  )
  let vertex_ref_request = vertex_ref.render(preview_input())

  let vertex_oauth: ai.Client = google.VertexClient.new(
    model = "gemini-preview",
    project_id = "preview-project",
    location = "global",
    headers = { "Authorization": "user-vertex-oauth" },
  )
  let vertex_oauth_request = vertex_oauth.render(preview_input())

  `gemini_key=${gemini_request.headers.get("x-goog-api-key") ?? "none"}`
    + `|gemini_header=${gemini_request.headers.get("x-user") ?? "none"}`
    + `|gemini_query=${gemini_request.url.includes("trace=kept")}`
    + `|gemini_body=${gemini_request.body.includes("preview_config")}`
    + `|gemini_user_key=${gemini_user_request.headers.get("x-goog-api-key") ?? "none"}`
    + `|vertex_marker=${vertex_request.url.includes("key=%3CPREVIEW%3E")}`
    + `|vertex_secret=${vertex_request.url.includes("vertex-secret")}`
    + `|vertex_header=${vertex_request.headers.get("x-user") ?? "none"}`
    + `|vertex_query=${vertex_request.url.includes("trace=kept")}`
    + `|vertex_body=${vertex_request.body.includes("preview_config")}`
    + `|vertex_ref_marker=${vertex_ref_request.url.includes("key=%3CPREVIEW%3E")}`
    + `|vertex_ref_projectless=${vertex_ref_request.url.includes("/v1/publishers/google/models/") && !vertex_ref_request.url.includes("/projects/")}`
    + `|vertex_oauth=${vertex_oauth_request.headers.get("authorization") ?? "none"}`
}
"#
    );

    assert_eq!(
        result_string(output),
        "gemini_key=<PREVIEW>|gemini_header=gemini|gemini_query=true|gemini_body=true|gemini_user_key=user-gemini-key|vertex_marker=true|vertex_secret=false|vertex_header=vertex|vertex_query=true|vertex_body=true|vertex_ref_marker=true|vertex_ref_projectless=true|vertex_oauth=user-vertex-oauth"
    );
}

#[tokio::test]
async fn bedrock_preview_uses_deterministic_region_without_profile_resolution() {
    let output = baml_test!(
        r#"
function PreviewShape() -> string {
  client: "openai/gpt-4o-mini"
  prompt: `hello`
}

function preview_input() -> ai.ModelTurnInput {
  let spec = PreviewShape@spec()
  ai.ModelTurnInput {
    prompt: spec.prompt_template,
    journal: ai.Journal.new(spec),
    toolbox: spec.tools(),
    output_type: spec.output_type(),
  }
}

function main() -> string {
  let inferred: ai.Client = aws.BedrockClient.new(
    model = "amazon.preview",
    profile = "baml-preview-profile-that-does-not-exist",
    headers = { "x-user": "bedrock" },
    query_params = { "trace": "kept" },
    request_body = baml.json.parse(`{"preview_config":"kept"}`),
  )
  let explicit: ai.Client = aws.BedrockClient.new(
    model = "amazon.preview",
    region = "us-west-2",
  )
  let inferred_request = inferred.render(preview_input())
  let explicit_request = explicit.render(preview_input())
  `inferred=${inferred_request.url.includes("bedrock-runtime.preview-region.amazonaws.com")}`
    + `|header=${inferred_request.headers.get("x-user") ?? "none"}`
    + `|query=${inferred_request.url.includes("trace=kept")}`
    + `|body=${inferred_request.body.includes("preview_config")}`
    + `|explicit=${explicit_request.url.includes("bedrock-runtime.us-west-2.amazonaws.com")}`
    + `|auth=${inferred_request.headers.get("authorization") ?? "none"}`
}
"#
    );

    assert_eq!(
        result_string(output),
        "inferred=true|header=bedrock|query=true|body=true|explicit=true|auth=none"
    );
}

#[tokio::test]
async fn provider_stream_open_preserves_timeout_and_cancellation() {
    let output = baml_test!(
        r#"
function chat_timeout() -> string {
  openai.internal._chat_stream_open_failure(
    "openai-chat",
    baml.errors.Timeout { message: "chat deadline", duration_ms: 37 },
  ) catch_all (e) {
    let timeout: baml.errors.Timeout => `chat_timeout:${timeout.duration_ms ?? -1}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function chat_cancelled() -> string {
  openai.internal._chat_stream_open_failure(
    "openai-chat",
    baml.panics.Cancelled { message: "chat stop" },
  ) catch_all (e) {
    let cancelled: baml.panics.Cancelled => `chat_cancelled:${cancelled.message}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function responses_timeout() -> string {
  openai.internal._openai_stream_open_failure(
    baml.errors.Timeout { message: "responses deadline", duration_ms: 41 },
  ) catch_all (e) {
    let timeout: baml.errors.Timeout => `responses_timeout:${timeout.duration_ms ?? -1}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function responses_cancelled() -> string {
  openai.internal._openai_stream_open_failure(
    baml.panics.Cancelled { message: "responses stop" },
  ) catch_all (e) {
    let cancelled: baml.panics.Cancelled => `responses_cancelled:${cancelled.message}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function anthropic_timeout() -> string {
  anthropic.internal._anthropic_stream_open_failure(
    baml.errors.Timeout { message: "anthropic deadline", duration_ms: 43 },
  ) catch_all (e) {
    let timeout: baml.errors.Timeout => `anthropic_timeout:${timeout.duration_ms ?? -1}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function google_timeout_source() -> string {
  let stream = google.internal._gm_route_sse_open(
    "google",
    () -> {
      throw baml.errors.Timeout { message: "google deadline", duration_ms: 47 }
    },
  )
  "unexpected"
}

function google_timeout() -> string {
  google_timeout_source() catch_all (e) {
    let timeout: baml.errors.Timeout => `google_timeout:${timeout.duration_ms ?? -1}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function google_cancelled_source() -> string {
  let stream = google.internal._gm_route_sse_open(
    "vertex",
    () -> {
      throw baml.panics.Cancelled { message: "vertex stop" }
    },
  )
  "unexpected"
}

function google_cancelled() -> string {
  google_cancelled_source() catch_all (e) {
    let cancelled: baml.panics.Cancelled => `google_cancelled:${cancelled.message}`,
    _ => `wrong:${e.to_string()}`,
  }
}

function main() -> string {
  [
    chat_timeout(),
    chat_cancelled(),
    responses_timeout(),
    responses_cancelled(),
    anthropic_timeout(),
    google_timeout(),
    google_cancelled(),
  ].join("|")
}
"#
    );

    assert_eq!(
        result_string(output),
        "chat_timeout:37|chat_cancelled:chat stop|responses_timeout:41|responses_cancelled:responses stop|anthropic_timeout:43|google_timeout:47|google_cancelled:vertex stop"
    );
}
