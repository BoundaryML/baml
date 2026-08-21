// openai.OpenRouterClient — OpenRouter's chat-completions gateway.
//
// A plain bearer-auth chat-completions endpoint at
// `https://openrouter.ai/api/v1`, keyed by OPENROUTER_API_KEY. Model ids are
// namespaced by upstream provider ("openai/gpt-4o-mini",
// "anthropic/claude-3.5-sonnet", …).
//
// App-attribution headers are not a special option: pass them through the
// ordinary `headers` field —
// `headers = {"HTTP-Referer": "https://myapp.com", "X-Title": "My App"}`.
// OpenRouter-specific body fields (`provider`, `route`, `transforms`) go
// through `request_body`.

class OpenRouterClient {
    model: string,
    api_key: ai.Credential?,
    base_url: ai.Credential?,

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
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
    ) -> OpenRouterClient {
        OpenRouterClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
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

    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, "OPENROUTER_API_KEY")
    }

    function resolved_base_url(self) -> string {
        ai.wire.resolve_credential(self.base_url, null) ?? "https://openrouter.ai/api/v1"
    }

    function compat(self) -> root.internal.ChatCompat {
        root.internal
            .ChatCompat
            .new(
                provider = "openrouter",
                base_url = self.resolved_base_url(),
                api_key_env = "OPENROUTER_API_KEY",
                api_key_required = true,
            )
    }

    function params(self, preview: bool = false) -> root.internal.ChatParams {
        root.internal
            .ChatParams
            .new(
                model = self.model,
                api_key = if (preview) {
                    null
                } else {
                    self.resolved_api_key()
                },
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
            `openrouter/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            openai.internal.chat_preview(self.compat(), self.params(preview = true), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.chat_invoke(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.chat_invoke_stream(self.compat(), self.params(), input) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
