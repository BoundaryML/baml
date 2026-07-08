// The legacy bridge (DCP §1.1, Phase B): `root.llm.Client` — the legacy
// client<llm> config object — IS a new-model provider, via the out-of-body
// implements blocks below. This is what lets the injected LLM-function
// `client` parameter retype to the existential `baml.ai.Provider` while
// (a) declared legacy configs remain the default value unchanged, and
// (b) legacy call-site overrides (`Foo(x, client = OtherClient)`) keep
// working alongside native ones (`client = Anthropic { … }`).
//
// v1 scope: the HttpProvider codec below bridges PRIMITIVE clients only.
// Legacy strategy configs (fallback / round-robin / retry-policy) route
// through the orchestrator when used as a declared/overridden client (the
// concrete `Client` arm in `call_llm_function` precedes capability
// negotiation), and throw typed `Unsupported` here if a capability driver
// tries to codec them directly — they lower to new-model combinators over
// per-member primitive clients in the Phase C step.

/// Best-effort metadata for bridged calls: the legacy parse path surfaces no
/// normalized wire metadata, so dimensions answer defaults.
class LegacyMeta {
    implements ResponseMeta {
        function finish_reason(self) -> string throws never {
            "unknown"
        }
        function usage(self) -> Usage throws never {
            Usage { input_tokens: 0, output_tokens: 0 }
        }
        function reasoning(self) -> string | Unavailable throws never {
            Unavailable { reason: "legacy bridge surfaces no reasoning" }
        }
        function logprobs(self) -> Logprob[] | Unavailable throws never {
            Unavailable { reason: "legacy bridge surfaces no logprobs" }
        }
        function citations(self) -> Citation[] | Unavailable throws never {
            Unavailable { reason: "legacy bridge surfaces no citations" }
        }
    }
}

implements Provider for root.llm.Client {}

implements HttpProvider for root.llm.Client {
    // The already-read response body (read once in `send`; `parse` and
    // `meta_of` share the string — `Response.text()` is not idempotent).
    type Body = string

    function build_request<T>(
        self,
        messages: ChatMessage[],
    ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
        if (self.client_type != root.llm.ClientType.Primitive) {
            throw root.errors.Unsupported {
                message: "the legacy codec bridges primitive clients; strategy configs lower to new-model combinators",
            };
        }
        let primitive = self.to_primitive_client() catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: client construction failed"] },
        };
        let ast = messages_to_prompt(messages);
        let specialized = primitive.specialize_prompt(ast) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: specialize failed"] },
        };
        primitive.build_request(specialized, reflect.type_of<T>()) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: build_request failed"] },
        }
    }

    function send(
        self,
        request: root.http.Request,
    ) -> string throws root.errors.UnknownError {
        let response = root.http.send(request) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: send failed"] },
        };
        response.text() catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: body read failed"] },
        }
    }

    function parse<T>(
        self,
        from: string,
    ) -> T throws root.errors.UnknownError {
        let primitive = self.to_primitive_client() catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: client construction failed"] },
        };
        primitive.parse<T>(from) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: parse failed"] },
        }
    }

    function meta_of(
        self,
        from: string,
    ) -> ResponseMeta throws never {
        LegacyMeta {}
    }

    // Override the interface default so a PORTED primitive (openai / anthropic /
    // google-ai, expressible config) routes through its BAML-native provider
    // class — the C.2 native routing, moved from `call_llm_function` into the
    // bridge so the desugared `drive_call` path inherits it. Everything else
    // falls through to the codec default above.
    function call_messages_with<T, V, E2>(
        self,
        messages: ChatMessage[],
        project: (ResponseMeta) -> V throws E2,
    ) -> CallResult<T, V> throws root.errors.CallError | root.errors.UnknownError | E2 {
        if (self.client_type == root.llm.ClientType.Primitive) {
            let primitive = self.to_primitive_client() catch (e) {
                _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: client construction failed"] },
            };
            match (native_provider_for(primitive)) {
                null => {},
                let np: Provider => {
                    match (np) {
                        let h: HttpProvider => {
                            return h.call_messages_with<T, V, E2>(messages, project);
                        },
                        _ => {},
                    }
                },
            }
        }
        let body = self.send(self.build_request<T>(messages));
        CallResult<T, V> { value: self.parse<T>(body), meta: project(self.meta_of(body)) }
    }
}

implements Streaming for root.llm.Client {
    // Streaming through the bridge (the piece Phase B left open): a ported
    // primitive whose native class supports Streaming routes natively; every
    // other primitive streams over the legacy SSE machinery (specialize from
    // messages -> legacy stream request -> accumulator `Stream`). Strategy
    // configs lower to combinators at declaration, so only primitives reach
    // this codec.
    function stream_messages<TStream, TFinal>(
        self,
        messages: ChatMessage[],
    ) -> root.llm.Stream<TStream, TFinal> throws root.errors.StreamError | root.errors.UnknownError {
        if (self.client_type != root.llm.ClientType.Primitive) {
            throw root.errors.Unsupported {
                message: "the legacy codec bridges primitive clients; strategy configs lower to new-model combinators",
            };
        }
        let primitive = self.to_primitive_client() catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: client construction failed"] },
        };
        match (native_provider_for(primitive)) {
            null => {},
            let np: Provider => {
                match (np) {
                    // Gemini's native class lacks Streaming — it falls through
                    // to the legacy SSE path below, like `stream_llm_function` did.
                    let s: Streaming => {
                        return s.stream_messages<TStream, TFinal>(messages);
                    },
                    _ => {},
                }
            },
        }
        let ast = messages_to_prompt(messages);
        let specialized = primitive.specialize_prompt(ast) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: specialize failed"] },
        };
        let request = primitive.build_request_stream(specialized, reflect.type_of<TFinal>()) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: build_request_stream failed"] },
        };
        let sse = root.http.fetch_sse(request) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: SSE connect failed"] },
        };
        self.__make_stream<TStream, TFinal>(sse) catch (e) {
            _ => throw root.errors.UnknownError { data: e, message: ["legacy bridge: stream construction failed"] },
        }
    }
}

/// Route a legacy PRIMITIVE client to its BAML-native provider class when the
/// provider is ported (openai / anthropic / google-ai) and the config is
/// expressible natively — query_params and finish-reason validation lists are
/// not, and keep the legacy request builder. Returns null otherwise.
///
/// This replaces the orchestrator's openai-only `_openai_delegation_ok` /
/// `_openai_from` special-case (DCP Phase C): the routing decision now lives
/// with the native classes, generalized to all ported providers, and is
/// consumed by `baml.llm.call_llm_function` / `stream_llm_function`.
/// Schema-append is disabled on every constructed provider: the rendered
/// template already carries (or deliberately omits) `${ctx.output_format}`.
function native_provider_for(primitive: root.llm.PrimitiveClient) -> Provider | null throws never {
    if (primitive.options.query_params.keys().length() != 0) {
        return null;
    }
    match (primitive.options.finish_reason_allow_list) {
        null => {},
        _ => {
            return null;
        },
    }
    match (primitive.options.finish_reason_deny_list) {
        null => {},
        _ => {
            return null;
        },
    }
    if (primitive.provider == "openai") {
        return OpenAi {
            model: primitive.options.model ?? "gpt-4o",
            api_key: primitive.options.api_key ?? "",
            base_url: primitive.options.base_url,
            extra_headers: primitive.options.headers,
            extra_body: primitive.options.request_body,
            append_output_schema: false,
        };
    }
    if (primitive.provider == "anthropic") {
        return Anthropic {
            model: primitive.options.model ?? "claude-sonnet-4-5",
            api_key: primitive.options.api_key,
            base_url: primitive.options.base_url,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: primitive.options.headers,
            extra_body: primitive.options.request_body,
            append_output_schema: false,
        };
    }
    if (primitive.provider == "google-ai") {
        return Gemini {
            model: primitive.options.model ?? "gemini-2.5-flash",
            api_key: primitive.options.api_key,
            base_url: primitive.options.base_url,
            extra_headers: primitive.options.headers,
            extra_body: primitive.options.request_body,
            append_output_schema: false,
        };
    }
    null
}
