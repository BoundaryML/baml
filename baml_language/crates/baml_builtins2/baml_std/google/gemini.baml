/// One Gemini safety filter setting, in the API's own spelling — pass a list
/// of these as a client's `safety_settings`:
///
///     google.GoogleClient.new(
///         model = "gemini-2.5-flash",
///         safety_settings = [
///             google.SafetySetting.new("HARM_CATEGORY_DANGEROUS_CONTENT", "BLOCK_ONLY_HIGH"),
///         ],
///     )
///
/// Categories are `HARM_CATEGORY_HARASSMENT`, `HARM_CATEGORY_HATE_SPEECH`,
/// `HARM_CATEGORY_SEXUALLY_EXPLICIT`, `HARM_CATEGORY_DANGEROUS_CONTENT` and
/// `HARM_CATEGORY_CIVIC_INTEGRITY`; thresholds are `BLOCK_NONE`,
/// `BLOCK_ONLY_HIGH`, `BLOCK_MEDIUM_AND_ABOVE`, `BLOCK_LOW_AND_ABOVE` and
/// `OFF`. The strings go on the wire verbatim, so a category this build has
/// never heard of still works.
class SafetySetting {
    category: string,
    threshold: string,

    function new(category: string, threshold: string) -> SafetySetting {
        SafetySetting { category: category, threshold: threshold }
    }
}

/// The Gemini Developer API client (`generativelanguage.googleapis.com`).
///
/// This client always talks to the Developer API. There is no environment
/// switch that re-points it at Vertex AI — the `GOOGLE_GENAI_USE_ENTERPRISE` /
/// `GOOGLE_GENAI_USE_VERTEXAI` flags of the google-genai SDK are deliberately
/// NOT honored here, because a credential-bearing client that changes endpoint,
/// auth scheme and billing account behind an ambient env var is a footgun. Use
/// the enterprise backend by naming it: `google.VertexClient.new(...)`, or the
/// `"vertex/<model>"` client shorthand.
class GoogleClient {
    model: string,
    // A literal string, `env.NAME` (a late-bound `baml.env.Ref` read at request
    // time), or null to resolve GOOGLE_API_KEY at request time. Construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: ai.Credential?,
    base_url: ai.Credential?,

    // ── passthrough (PLAN P0.3) ──────────────────────────────────────────
    // Deep-merged into the serialized request as the LAST step, so it can
    // reach anything the typed fields below do not model (thinkingConfig,
    // responseSchema, cachedContent, …). A null value DELETES that key.
    request_body: baml.json.json?,
    // Extra request headers, applied over the provider defaults and BEFORE
    // the credential header, which always wins.
    headers: map<string, string>?,
    // Extra URL query parameters, appended after `alt=sse` on the streaming
    // endpoint.
    query_params: map<string, string>?,

    // ── typed generationConfig ───────────────────────────────────────────
    // Each is sent only when non-null; `generationConfig` itself is omitted
    // when they all are.
    temperature: float?,
    max_output_tokens: int?,
    top_p: float?,
    top_k: int?,
    stop_sequences: string[]?,
    // e.g. ["TEXT", "IMAGE"] for the image-out models.
    response_modalities: string[]?,

    // Sent as `safetySettings`.
    safety_settings: SafetySetting[]?,

    function new(
        model: string = "gemini-2.5-flash",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
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
    ) -> GoogleClient {
        GoogleClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
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
        }
    }

    // The request-time credential: an explicit value, else the provider's
    // canonical one. A missing key is a CATCHABLE configuration error, not a
    // panic: `baml.env.get_or_panic` unwinds past the `catch_all` +
    // `ai.errors.normalize` wrapper below, so an unset variable would take the
    // process down instead of surfacing as an `ai.errors.Failure`.
    function resolved_api_key(self) -> string {
        if let k: string = ai.wire.resolve_credential(self.api_key, "GOOGLE_API_KEY") {
            return k;
        }
        throw ai.errors.InvalidRequest {
            provider: "google",
            status_code: null,
            detail: `google needs an API key: pass api_key (a literal or env.NAME), or set one of the variables it consulted — ${ai.wire.credential_sources(self.api_key, "GOOGLE_API_KEY")}`,
            raw_body: null,
        }
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            `google/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            google.internal.google_preview(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.gemini_invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.gemini_invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}
