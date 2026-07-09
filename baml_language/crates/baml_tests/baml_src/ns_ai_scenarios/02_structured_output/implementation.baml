// scenario 02 — structured output: ../../../../../llm-provider/ideas/scenarios/02-structured-output/implement.baml
// LIBRARY / std-lib-author code for the scenario. The four wire encodings the
// design hand-rolls (§2 OpenAI Chat response_format, §3 OpenAI Responses
// text.format, §4 Anthropic forced-tool, §5 Gemini responseSchema) are ALREADY
// the real stdlib providers — `baml.ai.OpenAiStrict`, `baml.ai.OpenAiResponses`,
// `baml.ai.Anthropic`, `baml.ai.Gemini` — each doing the T→build_request<T>→
// parse<T> lowering behind the one `HttpProvider` seam. So this file only carries
// the pieces the design ADDS on top of that seam that the stdlib does not already
// provide: the typed refusal (§1) and the strict→loose fallback combinator (§6).

// ---------------------------------------------------------------------------
// §1  A refusal is NOT malformed output — it is a typed failure.
//     Design class: `Refused`. On the real surface a refusal is a CallError
//     whose Failure axis answers `is_policy_refusal() -> true` (the four stdlib
//     providers each expose `is_policy_refusal` on their HTTP-error type). We
//     realize the design's concrete `Refused` as `S02Refused`: a first-class
//     CallError the parse layer / app / combinator can branch on BEFORE any
//     repair, so a decline is never fabricated into an object (the background's
//     core warning). The classifiers say what a refusal is NOT (not network,
//     not rate-limit, not parse) — it is its own concrete arm.
// ---------------------------------------------------------------------------
class S02Refused {
    category: string,
    message: string,
}

implements baml.errors.Failure for S02Refused {
    function is_retryable(self) -> bool { false }      // a decline is authoritative — never re-drive it
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { true }   // THE bit combinators/apps triage on
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
}

implements baml.errors.CallError for S02Refused {
    function is_network_error(self) -> bool { false }
    function is_rate_limit(self) -> bool { false }
    function is_parse_error(self) -> bool { false }     // a refusal is NOT a parse failure
}

// The same refusal can surface mid-stream, so it rides the StreamError channel too.
implements baml.errors.StreamError for S02Refused {
    function is_network_error(self) -> bool { false }
}

// A fake provider that always declines — the offline fixture for "refusal before
// parse". Its `parse<T>` is never reached: the decline is thrown on the call path
// so repair never runs on a body the model didn't produce.
class S02RefusingProvider {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["refusing provider has no request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["refusing provider does not send"] }
        }
        function parse<T>(
            self,
            from: string,
        ) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["refusing provider never parses"] }
        }
        function parse_meta(
            self,
            from: string,
        ) -> baml.ai.ResponseMeta throws never {
            baml.ai.LegacyMeta {}
        }

        // The refusal lands on the call path (like a real stop_reason/refusal slot),
        // never in the parsed value — so a caller must catch it, not decode it.
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            throw S02Refused { category: "refusal", message: "model declined to produce structured output" }
        }
    }
}

// ---------------------------------------------------------------------------
// §6  Combinator: S02StrictFallback — the standard schema-feature fallback.
//     Design class: `StrictFallback`. "try the strict primary → on a non-refusal
//     failure retry against the looser member → a refusal is AUTHORITATIVE and
//     must NOT be retried." The strict/non-strict choice lives INSIDE each
//     provider's build_request (the stdlib `OpenAiStrict` sends strict json_schema;
//     the loose member is a different, always-constrained engine), so — exactly as
//     the design's evaluation (c) flags — this combinator can only retry the WHOLE
//     call, holding two pre-configured members rather than flipping one knob.
//
//     It forwards the request/response capability, so it `implements HttpProvider`
//     (not the bare marker) and overrides the primitive `call_messages_with` (from
//     which `.call`/`.call_with` derive). It declares the SAME CallError channel as
//     its members — no narrow/widen/forward (error-model rule 4). The fluent
//     factories (`.with_retry`/`.fallback_to`) are inherited via `Provider`.
// ---------------------------------------------------------------------------
class S02StrictFallback {
    // primary: typically a strict-capable provider; loose: the non-strict retry.
    primary: baml.ai.Provider,
    loose: baml.ai.Provider,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        // No single honest request/response for a router, so the codec methods throw;
        // the real routing is in call_messages_with (mirrors stdlib `Fallback`).
        type Body = unknown

        function build_request<T>(
            self,
            messages: baml.ai.ChatMessage[],
        ) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["StrictFallback has no single request"] }
        }
        function send(
            self,
            request: baml.http.Request,
        ) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["StrictFallback does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["StrictFallback does not parse directly"] }
        }
        function parse_meta(
            self,
            from: unknown,
        ) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["StrictFallback has no meta"] }
        }

        // Route: try the strict primary; a refusal is authoritative and propagates,
        // anything else (schema rejected / 400 / transport) degrades to the loose
        // member. The projection rides along on the WINNING member's meta so
        // usage/sidecar is not dropped (the design's P1 fix).
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V>
            throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            match (self.primary) {
                let h: baml.ai.HttpProvider => {
                    let r0: baml.ai.CallResult<T, baml.ai.ResponseMeta> = h.call_messages_with<T, baml.ai.ResponseMeta, never>(
                        messages,
                        (mm: baml.ai.ResponseMeta) -> baml.ai.ResponseMeta { mm },
                    ) catch (e) {
                        let c: baml.errors.CallError => {
                            // an honest decline must NOT be retried — propagate it.
                            if (c.is_policy_refusal()) {
                                throw c;
                            }
                            // schema rejected / 400 / transport: fall to the loose member.
                            return self._loose_call<T, V, E2>(messages, project);
                        },
                        _ => {
                            return self._loose_call<T, V, E2>(messages, project);
                        },
                    };
                    baml.ai.CallResult<T, V> { value: r0.value, meta: project(r0.meta) }
                },
                // primary can't do request/response: go straight to loose.
                _ => self._loose_call<T, V, E2>(messages, project),
            }
        }
    }

    // Router to the loose member — also a capability match, so a non-http loose
    // member fails honestly with Unsupported rather than fabricating a call.
    function _loose_call<T, V, E2>(
        self,
        messages: baml.ai.ChatMessage[],
        project: (baml.ai.ResponseMeta) -> V throws E2,
    ) -> baml.ai.CallResult<T, V>
        throws baml.errors.CallError | baml.errors.UnknownError | E2 {
        match (self.loose) {
            let h: baml.ai.HttpProvider => h.call_messages_with<T, V, E2>(messages, project),
            _ => throw baml.errors.Unsupported { message: "StrictFallback.loose is not request/response" },
        }
    }
}
