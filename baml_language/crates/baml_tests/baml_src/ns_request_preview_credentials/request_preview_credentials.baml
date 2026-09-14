// Offline request-preview coverage for provider credentials and config.
// Preview (`render`) must never resolve credentials — env refs here are
// deliberately unset and profiles deliberately nonexistent: the preview is
// keyless by construction, while user-supplied auth headers pass through.
// Ported from the Rust `request_preview_credentials.rs` suite.

function PreviewShape() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `hello`
}

function _preview_input() -> ai.ModelTurnInput {
    let spec = PreviewShape@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

test "provider previews are keyless and preserve user headers" {
    let responses: ai.Client = openai.ResponsesClient.new(
        model = "gpt-preview",
        api_key = env.BAML_PREVIEW_OPENAI_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "responses" },
    );
    let anthropic_client: ai.Client = anthropic.Client.new(
        model = "claude-preview",
        api_key = env.BAML_PREVIEW_ANTHROPIC_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "anthropic" },
    );
    let gateway: ai.Client = vercel.AiGatewayImageClient.new(
        model = "bfl/flux-preview",
        api_key = env.BAML_PREVIEW_GATEWAY_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "gateway" },
    );
    let chat: ai.Client = openai.ChatClient.new(
        model = "gpt-preview",
        api_key = env.BAML_PREVIEW_CHAT_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "chat" },
    );
    let image: ai.Client = openai.ImageClient.new(
        model = "gpt-image-preview",
        api_key = env.BAML_PREVIEW_IMAGE_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "image" },
    );

    let responses_request = responses.render(_preview_input());
    assert.equal(responses_request.headers.get("authorization") ?? "none", "none");
    assert.equal(responses_request.headers.get("x-user") ?? "none", "responses");
    let anthropic_request = anthropic_client.render(_preview_input());
    assert.equal(anthropic_request.headers.get("x-api-key") ?? "none", "none");
    assert.equal(anthropic_request.headers.get("x-user") ?? "none", "anthropic");
    let gateway_request = gateway.render(_preview_input());
    assert.equal(gateway_request.headers.get("authorization") ?? "none", "none");
    assert.equal(gateway_request.headers.get("ai-gateway-auth-method") ?? "none", "none");
    assert.equal(gateway_request.headers.get("x-user") ?? "none", "gateway");
    let chat_request = chat.render(_preview_input());
    assert.equal(chat_request.headers.get("authorization") ?? "none", "none");
    assert.equal(chat_request.headers.get("x-user") ?? "none", "chat");
    let image_request = image.render(_preview_input());
    assert.equal(image_request.headers.get("authorization") ?? "none", "none");
    assert.equal(image_request.headers.get("x-user") ?? "none", "image")
}

test "user auth headers survive preview while configured keys do not" {
    let responses_configured: ai.Client = openai.ResponsesClient.new(
        model = "gpt-preview",
        api_key = "responses-secret",
        headers = { "Authorization": "user-responses", "x-config": "kept" },
    );
    let anthropic_configured: ai.Client = anthropic.Client.new(
        model = "claude-preview",
        api_key = "anthropic-secret",
        headers = { "X-Api-Key": "user-anthropic", "x-config": "kept" },
    );
    let gateway_configured: ai.Client = vercel.AiGatewayImageClient.new(
        model = "bfl/flux-preview",
        api_key = "gateway-secret",
        headers = {
            "Authorization": "user-gateway",
            "Ai-Gateway-Auth-Method": "user-method",
            "x-config": "kept",
        },
    );
    let chat_configured: ai.Client = openai.ChatClient.new(
        model = "gpt-preview",
        api_key = "chat-secret",
        headers = { "Authorization": "user-chat", "x-config": "kept" },
    );
    let image_automatic: ai.Client = openai.ImageClient.new(
        model = "gpt-image-preview",
        api_key = "image-secret",
        headers = { "x-config": "automatic" },
    );
    let image_configured: ai.Client = openai.ImageClient.new(
        model = "gpt-image-preview",
        api_key = "image-secret",
        headers = { "Authorization": "user-image", "x-config": "kept" },
    );

    let responses_user = responses_configured.render(_preview_input());
    assert.equal(responses_user.headers.get("authorization") ?? "none", "user-responses");
    assert.equal(responses_user.headers.get("x-config") ?? "none", "kept");
    let anthropic_user = anthropic_configured.render(_preview_input());
    assert.equal(anthropic_user.headers.get("x-api-key") ?? "none", "user-anthropic");
    assert.equal(anthropic_user.headers.get("x-config") ?? "none", "kept");
    let gateway_user = gateway_configured.render(_preview_input());
    assert.equal(gateway_user.headers.get("authorization") ?? "none", "user-gateway");
    assert.equal(gateway_user.headers.get("ai-gateway-auth-method") ?? "none", "user-method");
    assert.equal(gateway_user.headers.get("x-config") ?? "none", "kept");
    let chat_user = chat_configured.render(_preview_input());
    assert.equal(chat_user.headers.get("authorization") ?? "none", "user-chat");
    assert.equal(chat_user.headers.get("x-config") ?? "none", "kept");
    let image_automatic_request = image_automatic.render(_preview_input());
    assert.equal(image_automatic_request.headers.get("authorization") ?? "none", "none");
    assert.equal(image_automatic_request.headers.get("x-config") ?? "none", "automatic");
    let image_user = image_configured.render(_preview_input());
    assert.equal(image_user.headers.get("authorization") ?? "none", "user-image");
    assert.equal(image_user.headers.get("x-config") ?? "none", "kept")
}

test "google previews use deterministic secret-free markers" {
    let gemini: ai.Client = google.GeminiClient.new(
        model = "gemini-preview",
        api_key = env.BAML_PREVIEW_GOOGLE_KEY_THAT_IS_NEVER_SET,
        headers = { "x-user": "gemini" },
        query_params = { "trace": "kept" },
        request_body = baml.json.parse(`{"preview_config":"kept"}`),
    );
    let gemini_request = gemini.render(_preview_input());
    assert.equal(gemini_request.headers.get("x-goog-api-key") ?? "none", "<PREVIEW>");
    assert.equal(gemini_request.headers.get("x-user") ?? "none", "gemini");
    assert.is_true(gemini_request.url.includes("trace=kept"));
    assert.is_true(gemini_request.body.includes("preview_config"));

    let gemini_configured: ai.Client = google.GeminiClient.new(
        model = "gemini-preview",
        api_key = "gemini-config-secret",
        headers = { "X-Goog-Api-Key": "user-gemini-key" },
    );
    let gemini_user_request = gemini_configured.render(_preview_input());
    assert.equal(gemini_user_request.headers.get("x-goog-api-key") ?? "none", "user-gemini-key");

    let vertex: ai.Client = google.VertexClient.new(
        model = "gemini-preview",
        api_key = "vertex-secret",
        headers = { "x-user": "vertex" },
        query_params = { "trace": "kept" },
        request_body = baml.json.parse(`{"preview_config":"kept"}`),
    );
    let vertex_request = vertex.render(_preview_input());
    assert.is_true(vertex_request.url.includes("key=%3CPREVIEW%3E"));
    assert.is_true(!vertex_request.url.includes("vertex-secret"));
    assert.equal(vertex_request.headers.get("x-user") ?? "none", "vertex");
    assert.is_true(vertex_request.url.includes("trace=kept"));
    assert.is_true(vertex_request.body.includes("preview_config"));

    let vertex_ref: ai.Client = google.VertexClient.new(
        model = "gemini-preview",
        api_key = env.BAML_PREVIEW_VERTEX_KEY_THAT_IS_NEVER_SET,
    );
    let vertex_ref_request = vertex_ref.render(_preview_input());
    assert.is_true(vertex_ref_request.url.includes("key=%3CPREVIEW%3E"));
    assert.is_true(vertex_ref_request.url.includes("/v1/publishers/google/models/"));
    assert.is_true(!vertex_ref_request.url.includes("/projects/"));

    let vertex_oauth: ai.Client = google.VertexClient.new(
        model = "gemini-preview",
        project_id = "preview-project",
        location = "global",
        headers = { "Authorization": "user-vertex-oauth" },
    );
    let vertex_oauth_request = vertex_oauth.render(_preview_input());
    assert.equal(vertex_oauth_request.headers.get("authorization") ?? "none", "user-vertex-oauth")
}

test "bedrock preview uses deterministic region without profile resolution" {
    let inferred: ai.Client = aws.BedrockClient.new(
        model = "amazon.preview",
        profile = "baml-preview-profile-that-does-not-exist",
        headers = { "x-user": "bedrock" },
        query_params = { "trace": "kept" },
        request_body = baml.json.parse(`{"preview_config":"kept"}`),
    );
    let explicit: ai.Client = aws.BedrockClient.new(model = "amazon.preview", region = "us-west-2");
    let inferred_request = inferred.render(_preview_input());
    let explicit_request = explicit.render(_preview_input());
    assert.is_true(inferred_request.url.includes("bedrock-runtime.preview-region.amazonaws.com"));
    assert.equal(inferred_request.headers.get("x-user") ?? "none", "bedrock");
    assert.is_true(inferred_request.url.includes("trace=kept"));
    assert.is_true(inferred_request.body.includes("preview_config"));
    assert.is_true(explicit_request.url.includes("bedrock-runtime.us-west-2.amazonaws.com"));
    assert.equal(inferred_request.headers.get("authorization") ?? "none", "none")
}

// ── stream-open failures preserve host signals ───────────────────────────────

function _google_timeout_source() -> string {
    let stream = google.internal._gm_route_sse_open("google", () -> {
        throw baml.errors.Timeout { message: "google deadline", duration_ms: 47 }
    });
    "unexpected"
}

function _google_cancelled_source() -> string {
    let stream = google.internal._gm_route_sse_open("vertex", () -> {
        throw baml.panics.Cancelled { message: "vertex stop" }
    });
    "unexpected"
}

test "provider stream open preserves timeout and cancellation" {
    let chat_timeout = openai.internal._chat_stream_open_failure(
        "openai-chat",
        baml.errors.Timeout { message: "chat deadline", duration_ms: 37 },
    ) catch_all (e) {
        let timeout: baml.errors.Timeout => `chat_timeout:${timeout.duration_ms ?? -1}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(chat_timeout, "chat_timeout:37");

    let chat_cancelled = openai.internal._chat_stream_open_failure(
        "openai-chat",
        baml.panics.Cancelled { message: "chat stop" },
    ) catch_all (e) {
        let cancelled: baml.panics.Cancelled => `chat_cancelled:${cancelled.message}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(chat_cancelled, "chat_cancelled:chat stop");

    let responses_timeout = openai.internal._stream_open_failure(
        baml.errors.Timeout { message: "responses deadline", duration_ms: 41 },
    ) catch_all (e) {
        let timeout: baml.errors.Timeout => `responses_timeout:${timeout.duration_ms ?? -1}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(responses_timeout, "responses_timeout:41");

    let responses_cancelled = openai.internal._stream_open_failure(baml.panics.Cancelled { message: "responses stop" }) catch_all (e) {
        let cancelled: baml.panics.Cancelled => `responses_cancelled:${cancelled.message}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(responses_cancelled, "responses_cancelled:responses stop");

    let anthropic_timeout = anthropic.internal._stream_open_failure(
        baml.errors.Timeout { message: "anthropic deadline", duration_ms: 43 },
    ) catch_all (e) {
        let timeout: baml.errors.Timeout => `anthropic_timeout:${timeout.duration_ms ?? -1}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(anthropic_timeout, "anthropic_timeout:43");

    let google_timeout = _google_timeout_source() catch_all (e) {
        let timeout: baml.errors.Timeout => `google_timeout:${timeout.duration_ms ?? -1}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(google_timeout, "google_timeout:47");

    let google_cancelled = _google_cancelled_source() catch_all (e) {
        let cancelled: baml.panics.Cancelled => `google_cancelled:${cancelled.message}`,
        _ => `wrong:${e.to_string()}`,
    };
    assert.equal(google_cancelled, "google_cancelled:vertex stop")
}
