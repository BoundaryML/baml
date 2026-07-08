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
}
