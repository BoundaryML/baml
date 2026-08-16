// openai.ChatClient — one model turn against OpenAI's Chat Completions API
// (`POST https://api.openai.com/v1/chat/completions`).
//
// The sibling of `openai.ResponsesClient`: same provider, older and far more
// widely-cloned endpoint. Reach for this one when you want chat-completions
// semantics from OpenAI itself; for any other OpenAI-compatible server use
// `openai.GenericClient`, `openai.OllamaClient`, `openai.OpenRouterClient` or
// `openai.AzureClient`, which are the same wire core with different compat
// flags (`openai/ns_internal/chat.baml`).

class ChatClient {
    model: string,
    // A literal string, `env.NAME` (a late-bound `baml.env.Ref` read at request
    // time), or null to resolve OPENAI_API_KEY at request time. Construction
    // never touches env, so declaring/constructing a client is always safe.
    api_key: ai.Credential?,
    base_url: ai.Credential?,

    // Escape hatch: deep-merged into the serialized request as the LAST step.
    // A null value removes a key the client set (`{"max_tokens": null}`).
    request_body: baml.json.json?,
    // Extra request headers. Applied before the credential, so they cannot
    // shadow the endpoint's own auth header.
    headers: map<string, string>?,
    // Extra URL query parameters, percent-encoded.
    query_params: map<string, string>?,

    // First-class sampling parameters. Each is sent only when non-null.
    temperature: float?,
    max_tokens: int?,
    // The replacement for max_tokens on reasoning models (o-series, gpt-5).
    max_completion_tokens: int?,
    top_p: float?,
    stop: string[]?,
    seed: int?,

    function new(
        model: string = "gpt-4o-mini",
        api_key: ai.Credential? = null,
        base_url: ai.Credential? = null,
        request_body: baml.json.json? = null,
        headers: map<string, string>? = null,
        query_params: map<string, string>? = null,
        temperature: float? = null,
        max_tokens: int? = null,
        max_completion_tokens: int? = null,
        top_p: float? = null,
        stop: string[]? = null,
        seed: int? = null,
    ) -> ChatClient {
        ChatClient {
            model: model,
            api_key: api_key,
            base_url: base_url,
            request_body: request_body,
            headers: headers,
            query_params: query_params,
            temperature: temperature,
            max_tokens: max_tokens,
            max_completion_tokens: max_completion_tokens,
            top_p: top_p,
            stop: stop,
            seed: seed,
        }
    }

    // The request-time credential: the explicit value (a literal, or the value
    // of the variable an `env.NAME` ref points at), else the provider's
    // canonical variable. Null is reported as a configuration error when the
    // request is built, naming OPENAI_API_KEY.
    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, "OPENAI_API_KEY")
    }

    // The request-time base URL, or the provider default.
    function resolved_base_url(self) -> string {
        ai.wire.resolve_credential(self.base_url, null) ?? "https://api.openai.com/v1"
    }

    // The provider record: everything the shared core needs to know about this
    // endpoint. Only `stream_options` distinguishes first-party OpenAI from a
    // bare compatible server.
    function compat(self) -> root.internal.ChatCompat {
        root.internal
            .ChatCompat
            .new(
                provider = "openai-chat",
                base_url = self.resolved_base_url(),
                api_key_env = "OPENAI_API_KEY",
                api_key_required = true,
                send_stream_options = true,
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
                max_completion_tokens = self.max_completion_tokens,
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
            `openai-chat/${self.model}`
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
