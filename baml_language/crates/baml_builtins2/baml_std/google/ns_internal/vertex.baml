// root.VertexClient — the Vertex AI backend for Gemini.
//
// The request BODY is identical to the Gemini Developer API (everything in
// `gemini.baml` in this namespace is reused verbatim); Vertex differs in three
// places only:
//
//   1. the URL carries the project and the region:
//      https://{location}-aiplatform.googleapis.com/v1/projects/{project}
//          /locations/{location}/publishers/google/models/{model}:{verb}
//      with the region-less host `aiplatform.googleapis.com` for the `global`
//      location;
//   2. auth is a Google Cloud OAuth2 bearer token (minted by
//      `root.internal.gcp_access_token`, the one Rust seam), plus the
//      `x-goog-user-project` quota header — or, in express mode, a `key`
//      query parameter and no credentials at all;
//   3. Anthropic models live under a different publisher and speak the
//      Anthropic Messages wire format through `:rawPredict` /
//      `:streamRawPredict` (see `_vertex_anthropic_body`);
//   4. media URLs are NOT fetched and inlined: Vertex resolves them itself, so
//      an http(s)/gs:// URL rides through as `fileData.fileUri`
//      (`gemini_build_body(.., fetch_url = false)`).

// The OAuth2 scope every Vertex call is minted for.
function _vertex_scope() -> string {
    "https://www.googleapis.com/auth/cloud-platform"
}

// `vercel env pull` and friends store a credential document as a JSON *string*
// (`"{\"type\": …}"`). Unwrap that one level so both spellings work; anything
// else is returned untouched.
function _vertex_unwrap_json(document: string) -> string {
    let trimmed = document.trim();
    if (trimmed.starts_with("\"")) {
        baml.json.from_string<string>(trimmed) catch_all (e) {
            _ => document,
        }
    } else {
        document
    }
}

// The credential document to authenticate with, or null to run the Rust
// Application Default Credentials chain (GOOGLE_APPLICATION_CREDENTIALS, the
// well-known ADC file, the gcloud config file, the GCE metadata server).
//
// An explicitly configured source is used AS-IS: a broken value raises rather
// than silently cascading to the next one.
function _vertex_credentials(c: root.VertexClient) -> string? {
    if let inline: string = c.credentials_json {
        return _vertex_unwrap_json(inline);
    }
    if let path: string = c.credentials_file {
        let contents = baml.fs.read(path) catch_all (e) {
            _ => {
                throw ai.errors.InvalidRequest {
                    provider: "vertex",
                    status_code: null,
                    detail: `Vertex AI credentials file ${path} could not be read: ${e.to_string()}`,
                    raw_body: null,
                }
            },
        };
        return _vertex_unwrap_json(contents);
    }
    if let from_env: string = baml.env.get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT") {
        if (from_env.trim() != "") {
            return _vertex_unwrap_json(from_env);
        }
    }
    null
}

// Express mode: a Vertex API key, which replaces the whole OAuth2 flow with a
// `key` query parameter. Opt-in only — an ambient GOOGLE_API_KEY must never
// silently downgrade a project-scoped client.
function _vertex_express_key(c: root.VertexClient) -> string? {
    c.resolved_api_key()
}

// A `key` the caller put in `query_params` directly is express mode too (the
// spelling sys_llm and the old engine used).
function _vertex_key_in_query(c: root.VertexClient) -> bool {
    c.query_params?.get("key") != null
}

// Preview only needs to know that a key was configured. Inspecting the
// Credential union avoids reading an env.Ref whose value may be absent or
// secret; the live request still resolves it normally.
function _vertex_has_configured_express_key(c: root.VertexClient) -> bool {
    match (c.api_key) {
        let literal: string => true,
        let named: baml.env.Ref => true,
        null => false,
    }
}

function _vertex_is_express(c: root.VertexClient, preview: bool = false) -> bool {
    if (preview) {
        _vertex_has_configured_express_key(c) || _vertex_key_in_query(c)
    } else {
        _vertex_express_key(c) != null || _vertex_key_in_query(c)
    }
}

/// The region the request is routed to: the explicit option, else
/// `GOOGLE_CLOUD_LOCATION` / `GOOGLE_CLOUD_REGION`, else the multi-region
/// `global` endpoint (google-genai's default for the Vertex backend).
///
/// An explicitly-set empty string is honored, producing a deliberately broken
/// URL, so a misconfigured deployment surfaces instead of silently reaching a
/// different region.
function vertex_location(c: root.VertexClient) -> string {
    if let explicit: string = c.location {
        return explicit;
    }
    if let from_env: string = baml.env.get("GOOGLE_CLOUD_LOCATION") {
        if (from_env.trim() != "") {
            return from_env.trim();
        }
    }
    if let from_region: string = baml.env.get("GOOGLE_CLOUD_REGION") {
        if (from_region.trim() != "") {
            return from_region.trim();
        }
    }
    "global"
}

/// The project the request is billed to: the explicit option, else
/// `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT`, else whatever the credential
/// document and the google-auth chain report. Null means "not discoverable".
///
/// Express mode never consults credentials — a keyed request has none, and the
/// ADC chain would otherwise reach for the GCE metadata server.
/// Express mode is decided BEFORE the environment is consulted: an express key
/// is issued for the project-less express endpoint, and an ambient
/// `GOOGLE_CLOUD_PROJECT` (which every Cloud Run / GCE runtime sets by itself)
/// is not a configuration of THIS client — letting it through would silently
/// re-point a keyed client at a project-scoped URL the key cannot authorize.
/// An explicit `project_id` option still wins, since that IS configuration.
function vertex_project_id(c: root.VertexClient, preview: bool = false) -> string? {
    let explicit = ai.wire.resolve_credential(c.project_id, null);
    if let configured: string = explicit {
        return configured;
    }
    if (_vertex_is_express(c, preview = preview)) {
        return null;
    }
    if let from_env: string = baml.env.get("GOOGLE_CLOUD_PROJECT") {
        if (from_env.trim() != "") {
            return from_env.trim();
        }
    }
    if let legacy: string = baml.env.get("GCLOUD_PROJECT") {
        if (legacy.trim() != "") {
            return legacy.trim();
        }
    }
    if (preview) {
        // Do not enter the ADC/credential-file chain during a request preview.
        "preview-project"
    } else {
        root.internal.gcp_project_id(_vertex_credentials(c))
    }
}

// The publisher segment of the model path. Anthropic models are served by
// their own publisher with the Anthropic Messages wire format.
function _vertex_publisher(c: root.VertexClient) -> string {
    if (c.model.starts_with("claude")) {
        "anthropic"
    } else {
        "google"
    }
}

// The multi-region (data-residency) endpoints. `us` and `eu` are not regions
// in the `{region}-aiplatform.googleapis.com` sense: they are served by
// `aiplatform.{us|eu}.rep.googleapis.com`.
function _vertex_is_multi_region(location: string) -> bool {
    location == "us" || location == "eu"
}

// The `.../publishers/<publisher>/models` prefix a model name is appended to.
//
// An explicit base_url replaces the whole prefix (the caller owns the path),
// which is also how a test points the client at a mock server.
function _vertex_models_base(c: root.VertexClient, preview: bool = false) -> string {
    if let explicit: string = c.resolved_base_url() {
        return explicit;
    }
    let publisher = _vertex_publisher(c);
    let location = vertex_location(c);
    let host = if (location == "global") {
        "aiplatform.googleapis.com"
    } else if (_vertex_is_multi_region(location)) {
        // The data-residency multi-region endpoints have their own host shape;
        // `us-aiplatform.googleapis.com` does not resolve.
        `aiplatform.${location}.rep.googleapis.com`
    } else {
        `${location}-aiplatform.googleapis.com`
    };
    let project = vertex_project_id(c, preview = preview);
    if let resolved: string = project {
        `https://${host}/v1/projects/${resolved}/locations/${location}/publishers/${publisher}/models`
    } else if (_vertex_is_express(c, preview = preview)) {
        // Express mode has no project: the key selects the tenant, and the
        // request goes to the project-less global endpoint.
        `https://aiplatform.googleapis.com/v1/publishers/${publisher}/models`
    } else {
        throw ai.errors.InvalidRequest {
            provider: "vertex",
            status_code: null,
            detail: "Could not resolve project_id for Vertex AI. Set the client's project_id option, the GOOGLE_CLOUD_PROJECT env var, or supply credentials that carry a project_id.",
            raw_body: null,
        }
    }
}

// The method suffix: generateContent verbs for Gemini, rawPredict verbs for
// the Anthropic publisher.
function _vertex_method(c: root.VertexClient, stream: bool) -> string {
    if (_vertex_publisher(c) == "anthropic") {
        if (stream) {
            ":streamRawPredict"
        } else {
            ":rawPredict"
        }
    } else {
        root.internal.gemini_method(stream)
    }
}

/// The full endpoint for one call, query parameters included.
function vertex_url(c: root.VertexClient, stream: bool, preview: bool = false) -> string {
    let with_params = root.internal._gm_apply_query(
        `${_vertex_models_base(c, preview = preview)}/${c.model}${_vertex_method(c, stream)}`,
        c.query_params,
    );
    if (preview && _vertex_has_configured_express_key(c)) {
        let params: map<string, string> = { "key": "<PREVIEW>" };
        root.internal._gm_apply_query(with_params, params)
    } else if let key: string = _vertex_express_key(c) {
        let params: map<string, string> = { "key": key };
        root.internal._gm_apply_query(with_params, params)
    } else {
        with_params
    }
}

// Claude on Vertex is served by `publishers/anthropic` through `:rawPredict`,
// whose body is the Anthropic Messages format rather than generateContent.
// The body is built by the anthropic package's lowering (kept factored from
// URL/auth for exactly this reuse); Vertex additionally requires
// `anthropic_version` in the body and does NOT take `model` there — the model
// is named by the endpoint URL. `{"model": null}` is the deletion spelling of
// `merge_request_body`, applied over the Anthropic body the shared lowering
// built; a caller who needs it back can put it in `request_body`, which merges
// after these fields.
function _vertex_anthropic_body(
    c: root.VertexClient,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> string {
    let vertex_fields: baml.json.json = {
        "anthropic_version": "vertex-2023-10-16",
        "model": null,
    };
    let params = anthropic.internal.AnthropicBodyParams {
        model: c.model,
        max_tokens: c.max_output_tokens ?? 8192,
        temperature: c.temperature,
        top_p: c.top_p,
        top_k: c.top_k,
        stop_sequences: c.stop_sequences,
        thinking: null,
        tool_choice: null,
        // The caller's request_body merges over the Vertex constants, so an
        // explicit anthropic_version wins.
        request_body: ai.wire.merge_request_body(vertex_fields, c.request_body),
        client_id: c.id(),
    };
    anthropic.internal.anthropic_messages_body(params, input, stream, preview = preview)
}

// ai.ModelTurnInput -> one Vertex generateContent request.
function vertex_render(c: root.VertexClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _vertex_request(c, input, false, preview = false)
}

/// The request one Vertex `render` would send. Preview skips ADC, credential
/// file, and metadata-server access and uses a deterministic bearer marker.
function vertex_preview(c: root.VertexClient, input: ai.ModelTurnInput) -> baml.http.Request {
    _vertex_request(c, input, false, preview = true)
}

// The shared request builder: the Gemini body, the Vertex URL, and either a
// bearer token or an express key.
function _vertex_request(
    c: root.VertexClient,
    input: ai.ModelTurnInput,
    stream: bool,
    preview: bool = false,
) -> baml.http.Request {
    let body = if (_vertex_publisher(c) == "anthropic") {
        _vertex_anthropic_body(c, input, stream, preview = preview)
    } else {
        root.internal.gemini_build_body(
            input,
            c.id(),
            root.internal._gm_generation_config(
                c.temperature,
                c.max_output_tokens,
                c.top_p,
                c.top_k,
                c.stop_sequences,
                c.response_modalities,
            ),
            c.safety_settings,
            c.request_body,
            // Vertex resolves media URLs server-side: an http(s)/gs:// URL
            // goes through as `fileData.fileUri` instead of being fetched and
            // base64-inlined (sys_llm's `send_url` default for this backend),
            // which also keeps large media out of the inline size cap.
            fetch_url = false,
            preview = preview,
        )
    };
    let url = vertex_url(c, stream, preview = preview);
    let headers: map<string, string> = { "content-type": "application/json" };
    let with_user = root.internal._gm_apply_headers(headers, c.headers);
    if (_vertex_is_express(c, preview = preview)) {
        // The key rides the URL; nothing else to authenticate.
        null
    } else {
        if (preview) {
            if (with_user.get("authorization") == null) {
                let _ = with_user.set("authorization", "Bearer <PREVIEW>");
            }
        } else {
            let credentials = _vertex_credentials(c);
            let token = root.internal.gcp_access_token(credentials, _vertex_scope()) catch_all (e) {
                _ => {
                    throw ai.errors.InvalidRequest {
                        provider: "vertex",
                        status_code: null,
                        detail: `Vertex AI authentication failed: ${e.to_string()}`,
                        raw_body: null,
                    }
                },
            };
            let _ = with_user.set("authorization", `Bearer ${token}`);
            // google-auth parity (`Credentials.apply`): bill quota to the
            // configured project, unless the caller set the header themselves.
            if (with_user.get("x-goog-user-project") == null) {
                if let quota: string = root.internal.gcp_quota_project_id(credentials) {
                    let _ = with_user.set("x-goog-user-project", quota);
                }
            }
        }
        null
    };
    baml.http.Request { method: "POST", url: url, headers: with_user, body: body }
}

// The one internal entry point: render, send, normalize — one model turn.
// Claude models round-trip the Anthropic Messages format instead.
function vertex_invoke(c: root.VertexClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = vertex_render(c, input);
    if (_vertex_publisher(c) == "anthropic") {
        let resp = ai.wire.send_as<anthropic.internal.AnthropicResponse>(
            req,
            "vertex",
            request_timeout_ms = c.request_timeout_ms,
        );
        return anthropic.internal.anthropic_parse(resp);
    }
    let seq = input.journal.entries().length();
    let resp = ai.wire.send_as<root.internal.GmResponse>(
        req,
        "vertex",
        request_timeout_ms = c.request_timeout_ms,
    );
    root.internal.google_parse(resp, seq, provider = "vertex")
}

function _vertex_decode_batch(batch: string) -> ai.stream.StreamEvent[] {
    root.internal._gm_decode_batch(batch, "vertex")
}

// The streaming entry point: the same request against
// `:streamGenerateContent?alt=sse` (Gemini) or `:streamRawPredict` (Claude,
// which streams Anthropic Messages SSE and reuses that decoder), wrapped in
// an ai.TurnStream.
function vertex_invoke_stream(
    c: root.VertexClient,
    input: ai.ModelTurnInput,
) -> ai.stream.TurnStream {
    let req = _vertex_request(c, input, true, preview = false);
    let sse = root.internal._gm_open_sse(
        req,
        "vertex",
        request_timeout_ms = c.request_timeout_ms,
        time_to_first_token_timeout_ms = c.time_to_first_token_timeout_ms,
    );
    // Both decoders yield an explicit `TurnDone` (Anthropic's `message_stop`,
    // Gemini's `finishReason` fragment), so both streams are strict: a
    // connection cut mid-answer raises instead of reporting the partial text
    // as a completed turn.
    if (_vertex_publisher(c) == "anthropic") {
        let state = anthropic.internal.AnthStreamState.new();
        return ai.stream.TurnStream.from_sse(
            sse,
            (batch: string) -> {
                anthropic.internal.anthropic_decode_batch(batch, state)
            },
            require_terminal = "vertex",
        );
    }
    ai.stream.TurnStream.from_sse(sse, _vertex_decode_batch, require_terminal = "vertex")
}
