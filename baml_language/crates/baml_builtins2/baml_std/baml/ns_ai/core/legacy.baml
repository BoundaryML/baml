// The LegacyClient bridge (DCP §1.1, Phase B): the ONE consumer of the legacy
// PrimitiveClient Rust pipeline in the new provider model. A `client<llm>`
// config whose provider is not yet ported natively (azure, bedrock, vertex,
// ollama, …) — or whose config the native class can't express — lowers to
// this class, so the injected `client: baml.ai.Provider` parameter has a
// uniform type while the legacy request builder keeps doing the wire work.
//
// v1 scope: PRIMITIVE clients only. Legacy strategy configs (fallback /
// round-robin / retry-policy) are the lowering step's job — they become
// new-model combinators (`Fallback`/`RoundRobin`/`Retry`) over per-member
// bridges, so the legacy strategy loops are never re-entered from here.
// Streaming through the bridge lands with the lowering step.

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

/// A legacy `client<llm>` config as a new-model provider.
class LegacyClient {
    inner: root.llm.Client,

    implements Provider {}

    implements HttpProvider {
        // The already-read response body (read once in `send`; `parse` and
        // `meta_of` share the string — `Response.text()` is not idempotent).
        type Body = string

        function build_request<T>(
            self,
            messages: ChatMessage[],
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            if (self.inner.client_type != root.llm.ClientType.Primitive) {
                throw root.errors.Unsupported {
                    message: "LegacyClient bridges primitive clients; strategy configs lower to new-model combinators",
                };
            }
            let primitive = self.inner.to_primitive_client() catch (e) {
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
            let primitive = self.inner.to_primitive_client() catch (e) {
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
}
