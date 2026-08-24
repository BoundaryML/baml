// openai.OllamaClient — a local Ollama server through its OpenAI-compatible
// chat-completions endpoint.
//
// The whole client is four deviations from `openai.GenericClient`:
//   * the base URL defaults to `http://localhost:11434/v1`. The `/v1` is
//     load-bearing — Ollama serves the OpenAI-compatible API only under that
//     prefix, and sys_llm's `:11434` default 404s on every call (PLAN §5).
//   * no credential is required (and none is sent unless one is configured).
//   * the `system` role is allowed, like every other client here: Ollama passes
//     it straight through to the model's template.
//   * image URLs are fetched and inlined as base64. Ollama's OpenAI-compat
//     docs are explicit — "Base64 encoded image ✓ / Image URL ✗" — so an
//     `Image.from_url` prompt would otherwise reach a server that cannot read
//     it (`inline_image_urls`).

class OllamaClient {
    model: string,
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
    // Record request/response bodies (and raw SSE events) on the LLMCall wire
    // records delivered through on_event. Status, headers, and timing are
    // always recorded; this flag only gates the bodies.
    capture_wire: bool,

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
        capture_wire: bool = true,
    ) -> OllamaClient {
        OllamaClient {
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
            capture_wire: capture_wire,
        }
    }

    function resolved_api_key(self) -> string? {
        ai.wire.resolve_credential(self.api_key, null)
    }

    function resolved_base_url(self) -> string {
        ai.wire.resolve_credential(self.base_url, null) ?? "http://localhost:11434/v1"
    }

    function compat(self) -> root.internal.ChatCompat {
        root.internal
            .ChatCompat
            .new(
                provider = "ollama",
                base_url = self.resolved_base_url(),
                collapse_text_content = true,
                inline_image_urls = true,
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
            `ollama/${self.model}`
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            openai.internal.chat_preview(self.compat(), self.params(preview = true), input) catch_all (e) {
                _ => throw ai.errors.normalize(e),
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.chat_invoke(self.compat(), self.params(), input, capture_wire = self.capture_wire) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            root.internal.chat_invoke_stream(self.compat(), self.params(), input, capture_wire = self.capture_wire) catch_all (e) {
                _ => throw ai.errors.normalize_invoke(e),
            }
        }
    }
}
