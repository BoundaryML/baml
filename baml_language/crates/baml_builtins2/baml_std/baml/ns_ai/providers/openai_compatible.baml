// The OpenAI-compatible generic provider (backlog "provider diversity" item;
// scenario 28): the Chat Completions wire shape against ANY base_url — proxies,
// gateways, and local runtimes (vLLM, llama.cpp server, LM Studio, Ollama's
// /v1 compat endpoint). The request/parse pipeline is DELEGATED to an internal
// `OpenAi` value so the two providers cannot drift; what differs is a required
// `base_url` and a TYPED `auth` field instead of a hardwired Bearer key.

/// No authentication (local runtimes).
class NoAuth {}

/// `Authorization: Bearer <token>` — the OpenAI convention, most gateways.
class BearerAuth {
    token: string,
}

/// A custom auth header (e.g. `api-key: …` for some proxies).
class HeaderAuth {
    name: string,
    value: string,
}

/// An OpenAI-compatible Chat Completions endpoint at an arbitrary base URL.
class OpenAiCompatible {
    model: string,
    /// The endpoint base, e.g. `http://localhost:8000/v1` — required; pointing
    /// somewhere else is this provider's whole reason to exist.
    base_url: string,
    /// How to authenticate: `BearerAuth { token }`, `HeaderAuth { name, value }`,
    /// or `NoAuth {}` for unauthenticated local runtimes.
    auth: BearerAuth | HeaderAuth | NoAuth,
    /// Extra HTTP headers merged onto the request (proxies, tracing).
    extra_headers: map<string, string>?,
    /// Extra top-level body params merged into the request JSON (e.g. temperature).
    extra_body: map<string, unknown>?,
    /// Append T's output schema to the prompt (default true) — same SAP
    /// convention as `OpenAi`.
    append_output_schema: bool?,

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        // Raw response body text, read once in `send` (same convention as OpenAi).
        type Body = string

        function build_request<T>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.http.Request throws root.errors.UnknownError {
            // Delegate URL/body construction to the OpenAi builder, then swap in
            // the auth-derived headers.
            let r = self._inner().build_request<T>(messages);
            root.http.Request {
                method: r.method,
                url: r.url,
                headers: self._headers(),
                body: r.body,
            }
        }

        function send(
            self,
            request: root.http.Request,
        ) -> string throws root.errors.CallError | root.errors.UnknownError {
            // Same send + error normalization (a non-2xx is a typed
            // `OpenAiHttpError`, which implements the CallError channel).
            self._inner().send(request)
        }

        function parse<T>(
            self,
            from: string,
        ) -> T throws root.errors.UnknownError {
            self._inner().parse<T>(from)
        }

        function parse_meta(
            self,
            from: string,
        ) -> root.ai.ResponseMeta throws never {
            self._inner().parse_meta(from)
        }
    }

    implements root.ai.Streaming {
        // Chat-completions streaming (`"stream": true`) against the same base_url;
        // SSE accumulation reuses the host accumulator via an `openai-generic`
        // shorthand primitive (identical delta shape to OpenAI).
        function stream_messages<TStream, TFinal>(
            self,
            messages: root.ai.ChatMessage[],
        ) -> root.llm.Stream<TStream, TFinal> throws root.errors.UnknownError {
            let inner = self._inner();
            let schema: string = root.llm.render_output_format(reflect.type_of<TFinal>());
            let req = root.http.Request {
                method: "POST",
                url: self.base_url + "/chat/completions",
                headers: self._headers(),
                body: inner._request_body(inner._messages_for(messages, schema), true),
            };
            let sse = root.http.fetch_sse(req) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-compatible fetch_sse failed"] }
                },
            };
            let primitive = root.llm.from_shorthand("openai-generic/" + self.model) catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-compatible stream setup failed"] }
                },
            };
            let acc = primitive.new_stream_accumulator() catch (e) {
                _ => {
                    throw root.errors.UnknownError { data: e, message: ["openai-compatible accumulator failed"] }
                },
            };
            let cache = root.llm.StreamCache.new(reflect.type_of<TStream>(), reflect.type_of<TFinal>());
            root.llm.Stream<TStream, TFinal> {
                _client: primitive,
                _acc: acc,
                _sse: sse,
                _cache: cache,
            }
        }
    }

    // ── private helpers ──

    // The delegate that owns the Chat Completions wire shape. Its api_key is
    // unused: `build_request`/`stream_messages` replace the headers wholesale
    // with `_headers()` below.
    function _inner(self) -> OpenAi throws never {
        OpenAi {
            model: self.model,
            api_key: "",
            base_url: self.base_url,
            extra_headers: null,
            extra_body: self.extra_body,
            append_output_schema: self.append_output_schema,
        }
    }

    // Content-type + the typed auth + any extra_headers.
    function _headers(self) -> map<string, string> throws never {
        let headers: map<string, string> = { "Content-Type": "application/json" };
        match (self.auth) {
            let b: BearerAuth => {
                headers.set("Authorization", "Bearer " + b.token);
            },
            let h: HeaderAuth => {
                headers.set(h.name, h.value);
            },
            _ => {
            },
        }
        match (self.extra_headers) {
            let extra: map<string, string> => {
                for (let k in extra.keys()) {
                    headers.set(k, extra.get(k) ?? "");
                }
            },
            _ => {
            },
        }
        headers
    }
}
