// scenario 32 — observability: ../../../../../llm-provider/ideas/scenarios/32-observability/README.md
//   implementation.baml — LIBRARY-author-analog fakes for the OFFLINE tier.
//
// The design rides observability on the VALUE path (not a host span exporter):
//   - per-call meta projection via `call_with` / the `Foo$with` companion,
//   - chain-wide provenance/usage via `UsageMeter` + `Traced` + `.traced(meter)`,
//   - `AggregateMeta` summing usage over the calls that actually ran,
//   - `log.*` as the structured event stream (the "exhaust").
//
// The design's teaching point #4 is USAGE NORMALIZATION: every provider buries
// token counts under different keys (OpenAI: prompt/completion; Anthropic:
// input/output + cache), yet a span needs ONE shape. On the real surface each
// provider's `ResponseMeta` normalizes lazily into ONE `baml.ai.Usage`
// (input_tokens/output_tokens) — the two fakes below encode their tokens under
// DIVERGENT internal field names and both project the same normalized Usage.


// -- Divergent per-provider metadata, normalized to ONE baml.ai.Usage. --------

/// OpenAI-style: tokens live under `prompt` / `completion` (Chat-Completions).
class s32_OpenAiStyleMeta {
    prompt: int,
    completion: int,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.prompt, output_tokens: self.completion }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai chat" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai chat" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai chat" }
        }
    }
}

/// Anthropic-style: tokens live under `input` / `output` PLUS a `cache_read`
/// split. The real `baml.ai.Usage` is 2-field (no cache slot — a design
/// divergence, see the report), so normalization FOLDS `cache_read` into
/// `input_tokens`: the span still attributes every billed input token.
class s32_AnthropicStyleMeta {
    input: int,
    output: int,
    cache_read: int,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "end_turn" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.input + self.cache_read, output_tokens: self.output }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic" }
        }
    }
}


// -- A metered echo provider carrying any ResponseMeta on the value path. ------
// Answers `reply` (SAP-parsed into T) and projects `meta` out of the SAME codec
// pass as the value — usage rides WITH the answer (the P1 fix: no stateful
// `last_usage()` stash, so a Fallback forwards the winner's meta, no race).

class s32_MeteredEcho {
    reply: string,
    meta: baml.ai.ResponseMeta,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered echo has no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["metered echo does not send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["metered echo parse failed"] },
            }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            self.meta
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["metered echo parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(self.meta) }
        }
    }
}
