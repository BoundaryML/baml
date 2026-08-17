// openai.GenericClient — any OpenAI-compatible chat-completions endpoint:
// vLLM, LM Studio, TGI, llama.cpp, Together, Fireworks, a corporate gateway.
//
// Two things separate it from `openai.ChatClient`:
//   * `base_url` is REQUIRED. An unset base URL is a configuration error, not a
//     silent fallback to api.openai.com — sending a private prompt to OpenAI
//     because a variable was unset is the worse failure (PLAN §5).
//   * all-text messages are sent as `content: "…"` rather than
//     `content: [{"type":"text",…}]`. Many compatible servers reject the array
//     form outright; media-bearing messages still use parts.
// It also sends no `stream_options` (undocumented outside first-party OpenAI)
// and requires no credential — a local server usually has none.

class GenericClient {
    model: string,
    // Required: the endpoint's base, up to and including any version prefix,
    // e.g. "http://localhost:8000/v1". May be a literal or an `env.NAME` ref
    // read at request time. An `env.NAME` whose variable is unset is the same
    // configuration error as no base_url at all.
    base_url: ai.Credential?,
    api_key: ai.Credential?,

    request_body: baml.json.json?,
    headers: map<string, string>?,
    query_params: map<string, string>?,

    temperature: float?,
    max_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,

    function new(
        model: string,
        base_url: ai.Credential? = null,
        api_key: ai.Credential? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
    ) -> GenericClient {
        GenericClient {
            model: model,
            base_url: base_url,
            api_key: api_key,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            temperature: temperature,
            max_tokens: max_tokens,
            top_p: top_p,
            stop: stop,
            seed: seed,
        }
    }

    // No canonical variable: an unauthenticated request is the normal case for
    // a self-hosted server, so a missing key is not an error here.
    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, null)
    }

    // Throws `ai.errors.InvalidRequest` when no base_url resolves. A
    // `base_url = env.NAME` naming an UNSET variable resolves to null and lands
    // here too, so it reports the same clear configuration error rather than a
    // mystery request to nowhere.
    function resolved_base_url(self) -> string {
        if let explicit: string = ai.wire.resolve_credential(self.base_url, null) {
            return explicit;
        }
        // A `base_url = env.NAME` whose variable is not set must not degrade
        // into a mystery request, so it raises the same error a plainly-unset
        // base_url does — plus the names it consulted, so the ref's own
        // variable is reported.
        throw root.internal._reject(
            "openai-generic",
            `openai.GenericClient requires base_url (a literal or env.NAME naming a set variable): the endpoint of the OpenAI-compatible server, e.g. "http://localhost:8000/v1". Checked: ${ai.wire.credential_sources(self.base_url, null)}`,
        )
    }

    function compat(self) -> root.internal.ChatCompat {
        root.internal.ChatCompat.new(
            provider = "openai-generic",
            base_url = self.resolved_base_url(),
            collapse_text_content = true,
        )
    }

    function params(self) -> root.internal.ChatParams {
        root.internal.ChatParams.new(
            model = self.model,
            api_key = self.resolved_api_key(),
            temperature = self.temperature,
            max_tokens = self.max_tokens,
            top_p = self.top_p,
            stop = self.stop,
            seed = self.seed,
            headers = self.headers,
            query_params = self.query_params,
            request_body = self.request_body,
        )
    }

    implements ai.Client {
        function id(self) -> string {
            `openai-generic/${self.model}`
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.chat_invoke(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.chat_invoke_stream(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }
    }
}
