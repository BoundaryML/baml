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
    // null resolves OPENAI_API_KEY at request time; construction never
    // touches env, so declaring/constructing a client is always safe.
    api_key: string?,
    base_url: string?,
    // Names of environment variables to read INSTEAD, at request time.
    // A `client Name = ...` declaration is evaluated during `$init`, which
    // cannot run io sysops — so a declaration can never read env itself.
    // Naming the variable keeps the declaration pure while still letting a
    // deployment (or a replay harness) point the client somewhere else.
    api_key_env: string?,
    base_url_env: string?,

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

    function new(
        model: string = "gpt-4o-mini",
        api_key: string? = null,
        base_url: string? = null,
        api_key_env: string? = null,
        base_url_env: string? = null,
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
    ) -> ResponsesClient {
        ResponsesClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            api_key_env: api_key_env,
            base_url_env: base_url_env,
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
        }
    }

    // The request-time credential: an explicit value, else the named env
    // var, else the provider's canonical one.
    function resolved_api_key(self) -> string {
        if let k: string = self.api_key {
            return k;
        }
        if let name: string = self.api_key_env {
            if let v: string = baml.env.get(name) {
                return v;
            }
        }
        baml.env.get_or_panic("OPENAI_API_KEY")
    }

    // The request-time base URL, or null for the provider default.
    function resolved_base_url(self) -> string? {
        if let u: string = self.base_url {
            return u;
        }
        if let name: string = self.base_url_env {
            return baml.env.get(name);
        }
        null
    }

    implements ai.Client {
        function id(self) -> string {
            `openai/${self.model}`
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.invoke_stream(self, input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}
