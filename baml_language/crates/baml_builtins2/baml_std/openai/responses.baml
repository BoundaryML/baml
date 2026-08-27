// The OpenAI Responses API client (POST {base_url}/responses).
//
// Options come in three groups:
//   * credentials/endpoint — `api_key`/`base_url` (or the `*_env` names read at
//     request time);
//   * first-class typed request parameters — `temperature`, `top_p`,
//     `max_output_tokens`, `reasoning_effort`, `reasoning_summary`,
//     `verbosity`, `parallel_tool_calls`, `store`. Each one is lowered into the
//     typed request class only when it is non-null (PLAN §1.5);
//   * passthrough — `request_body` (deep-merged over the serialized typed
//     request as the LAST step, `null` deletes a key), `headers` and
//     `query_params`.
//
// Structured output stays SAP-based: the schema rides in the prompt text and
// `text.format` is deliberately never set (research report G15). A caller who
// wants native JSON-schema output sets it through `request_body`.
//
// Knowingly dropped, on top of PLAN §6's list: the engine's
// `finish_reason_allow_list` / `finish_reason_deny_list` (research row G10).
// There is no config surface for them here and none is planned — the typed
// `ai.content.StopReason` plus the runner's Refused / MaxTokens enforcement
// covers what the lists were used for, and a caller who wants a stricter policy
// writes it over the returned turn (or wraps the client) rather than
// configuring a string list of provider finish reasons.
class ResponsesClient {
    model: string,
    // A literal string, `env.NAME` (a late-bound `baml.env.Ref`, read at
    // request time), or null to resolve OPENAI_API_KEY at request time.
    // Construction never touches env, so declaring/constructing a client is
    // always safe — and a `Ref` keeps the secret out of the client value.
    api_key: ai.Credential?,
    base_url: ai.Credential?,
    // Native request deadline, including a streaming response body. Null or a
    // non-positive value is unbounded; browser/Wasm does not enforce it.
    request_timeout_ms: int?,
    // Native streaming deadline from SSE open until the first parsed event.
    time_to_first_token_timeout_ms: int?,

    // --- typed request parameters (omitted from the body when null) --------
    temperature: float?,
    top_p: float?,
    max_output_tokens: int?,
    // `reasoning.effort`: "none" | "minimal" | "low" | "medium" | "high" |
    // "xhigh" | "max" — an unvalidated passthrough, so a value the API adds
    // later works without a stdlib change.
    reasoning_effort: string?,
    // `reasoning.summary`: "auto" | "concise" | "detailed". The remaining
    // `reasoning` subfields (`mode`, `context`, …) ride through `request_body`.
    reasoning_summary: string?,
    // `text.verbosity`: "low" | "medium" | "high".
    verbosity: string?,
    parallel_tool_calls: bool?,
    // Server-side retention of the response. Defaults to false: the API's own
    // default is `true`, which silently persists every response.
    store: bool,

    // --- passthrough -------------------------------------------------------
    /// Deep-merged over the serialized typed request as the last step before
    /// the body is stringified. A `null` value DELETES that key, so
    /// `{"store": null}` drops the `store: false` this client sends.
    request_body: baml.json.json?,
    /// Extra request headers, applied (lowercased) before authorization.
    headers: map<string, string>?,
    /// Extra URL query parameters, percent-encoded onto the endpoint.
    query_params: map<string, string>?,
    // Record request/response bodies (and raw SSE events) on the LLMCall wire
    // records delivered through on_event. Status, headers, and timing are
    // always recorded; this flag only gates the bodies.
    capture_wire: bool,

    function new(
        model: string = "gpt-4o-mini",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        request_timeout_ms: int? = null,
        time_to_first_token_timeout_ms: int? = null,
        temperature: float? = null,
        top_p: float? = null,
        max_output_tokens: int? = null,
        reasoning_effort: string? = null,
        reasoning_summary: string? = null,
        verbosity: string? = null,
        parallel_tool_calls: bool? = null,
        store: bool = false,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        capture_wire: bool = true,
    ) -> ResponsesClient {
        ResponsesClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            request_timeout_ms: request_timeout_ms,
            time_to_first_token_timeout_ms: time_to_first_token_timeout_ms,
            temperature: temperature,
            top_p: top_p,
            max_output_tokens: max_output_tokens,
            reasoning_effort: reasoning_effort,
            reasoning_summary: reasoning_summary,
            verbosity: verbosity,
            parallel_tool_calls: parallel_tool_calls,
            store: store,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            capture_wire: capture_wire,
        }
    }

    // The request-time credential: an explicit value (a literal, or the value
    // of the variable an `env.NAME` ref points at), else the provider's
    // canonical one. Nothing here runs before the request.
    //
    // A missing key is a CATCHABLE configuration error, not a panic:
    // `baml.env.get_or_panic` unwinds past the `catch_all` +
    // `ai.errors.normalize` wrapper below, so an unset variable would take the
    // process down instead of surfacing as an `ai.errors.Failure`.
    function resolved_api_key(self) -> string {
        if let k: string = ai.wire.resolve_credential(self.api_key, "OPENAI_API_KEY") {
            return k;
        }
        throw ai.errors.InvalidRequest {
            provider: "openai",
            status_code: null,
            detail: `openai needs an API key: pass api_key (a literal or env.NAME), or set one of the variables it consulted — ${ai.wire.credential_sources(self.api_key, "OPENAI_API_KEY")}`,
            raw_body: null,
        }
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        ai.wire.resolve_credential(self.base_url, null)
    }

    implements ai.Client {
        function id(self) -> string {
            `openai/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            openai.internal.openai_preview(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
