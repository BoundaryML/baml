/// Gemini on Vertex AI: the same models as `google.GoogleClient`, reached
/// through a Google Cloud project with Google Cloud credentials.
///
/// Three ways to authenticate, in the order the client tries them:
///
///   * `credentials_json` — an inline credential document (service account,
///     authorized user, workload identity federation, impersonated service
///     account);
///   * `credentials_file` — a path to one, plus the
///     `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env var holding a document
///     inline;
///   * nothing — Application Default Credentials
///     (`GOOGLE_APPLICATION_CREDENTIALS`, the well-known ADC file, the gcloud
///     config file, the GCE metadata server).
///
/// `api_key` selects Vertex AI **express mode** instead: the key rides the URL
/// as `?key=…`, no credentials are read, and the request goes to the
/// project-less global endpoint unless a project is configured.
class VertexClient {
    model: string,
    // null resolves from GOOGLE_CLOUD_PROJECT / the credential document at
    // request time; construction never touches env or the filesystem.
    project_id: ai.Credential?,
    // null resolves from GOOGLE_CLOUD_LOCATION / GOOGLE_CLOUD_REGION, else
    // the multi-region `global` endpoint.
    location: string?,
    // An inline credential document, or the path to one. Whichever is set is
    // used as-is: a broken value raises instead of falling back.
    credentials_json: string?,
    credentials_file: string?,
    // Express-mode API key (opt-in; there is deliberately no ambient env
    // fallback, so an unrelated GOOGLE_API_KEY cannot downgrade a
    // project-scoped client).
    // A literal string or `env.NAME` (a late-bound `baml.env.Ref` read at
    // request time); null keeps OAuth2 credentials as the path.
    api_key: ai.Credential?,
    // Replaces the whole `…/publishers/google/models` prefix; the caller owns
    // the path from there on.
    base_url: ai.Credential?,
    // Native request deadline, including a streaming response body. Null or a
    // non-positive value is unbounded; browser/Wasm does not enforce it.
    request_timeout_ms: int?,
    // Native streaming deadline from SSE open until the first parsed event.
    time_to_first_token_timeout_ms: int?,

    // ── passthrough (PLAN P0.3) ──────────────────────────────────────────
    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    // ── typed generationConfig ───────────────────────────────────────────
    temperature: float?,
    max_output_tokens: int?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    response_modalities: string[]?,

    safety_settings: SafetySetting[]?,

    // Record request/response bodies (and raw SSE events) on the LLMCall wire
    // records delivered through on_event. Status, headers, and timing are
    // always recorded; this flag only gates the bodies.
    capture_wire: bool,

    function new(
        model: string = "gemini-2.5-flash",
        project_id: ai.Credential? = null,
        location: string? = null,
        credentials_json: string? = null,
        credentials_file: string? = null,
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        request_timeout_ms: int? = null,
        time_to_first_token_timeout_ms: int? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_output_tokens: int? = null,
        top_p: float? = null,
        top_k: int? = null,
        stop_sequences: string[]? = null,
        response_modalities: string[]? = null,
        safety_settings: SafetySetting[]? = null,
        capture_wire: bool = true,
    ) -> VertexClient {
        VertexClient {
            model: model,
            project_id: project_id,
            location: location,
            credentials_json: credentials_json,
            credentials_file: credentials_file,
            api_key: api_key,
            base_url: base_url,
            request_timeout_ms: request_timeout_ms,
            time_to_first_token_timeout_ms: time_to_first_token_timeout_ms,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            temperature: temperature,
            max_output_tokens: max_output_tokens,
            top_p: top_p,
            top_k: top_k,
            stop_sequences: stop_sequences,
            response_modalities: response_modalities,
            safety_settings: safety_settings,
            capture_wire: capture_wire,
        }
    }

    /// The express-mode key, or null for OAuth2. Unlike the other providers
    /// this may legitimately be absent: credentials are the normal path.
    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, null)
    }

    /// The request-time base URL, or null to build one from project + location.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            `vertex/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            google.internal.vertex_preview(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.vertex_invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.vertex_invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
