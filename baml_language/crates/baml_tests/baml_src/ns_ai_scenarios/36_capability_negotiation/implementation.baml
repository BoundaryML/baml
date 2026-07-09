// scenario 36 — capability negotiation: ../../../../../llm-provider/ideas/scenarios/36-capability-negotiation/README.md
//   implementation.baml — STD-LIB / library-author code. Fixtures + a negotiation
//   wrapper on the REAL `baml.ai` surface.
//
// The design's central claim: capability negotiation is FIRST-CLASS. There are two
// distinct negotiation layers, and this file realizes both against the real model:
//
//   (1) BINARY, behavioral — "can this provider stream / run tools / go realtime AT
//       ALL". Answered by a runtime `match (p) { let s: Streaming => … }` over the
//       existential `baml.ai.Provider` marker (a client returns the marker; the
//       capability set IS the type). The `_` arm decides degrade-vs-error. This is
//       what the generated companions (`drive_call`/`drive_stream`) already do.
//
//   (2) GRADED — "supported, but degraded / only for THIS payload". A `match` is
//       binary; support is not. The real `Capabilities` interface returns the
//       three-valued `Support` lattice (Yes/No/Maybe). `Yes`=send, `No`=don't
//       bother, `Maybe`=the runtime probe is the truth. `s36_Negotiated` below is
//       the bridge: it reads the lattice and, on `Maybe`, actually PROBES and
//       degrades on a typed schema rejection.
//
// The consumer is a plain LLM function (`s36_Extract`, usage.baml) whose `client`
// swaps between these fixtures — negotiation lives in the drivers/wrapper, never in
// the function body.
//
// Reuses the shared offline fakes from ../common (EchoProvider, NoCapProvider).
// Real surface referenced: baml.ai.{Provider, HttpProvider, Streaming, Capabilities,
// Support, Tools, ChatMessage, ResponseMeta, CallResult, LegacyMeta},
// baml.errors.{CallError, StreamError, Failure, Unsupported, UnknownError}.


// ── The typed record the app extracts. ──────────────────────────────────────
class s36_Report {
    subject: string,
    verdict: string,
}


// ── The typed face of a runtime structured-output rejection. ─────────────────
// Distinct from a transport blip so a Maybe-probe can decide "the SCHEMA was the
// problem -> downgrade" vs "the network was the problem -> propagate". Implements
// Failure + CallError (is_parse_error -> true); a streamed structured output can be
// rejected mid-stream too, so it also implements StreamError.
class s36_SchemaRejected {
    message: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.CallError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { true }
    }
    implements baml.errors.StreamError {
        function is_network_error(self) -> bool { false }
    }
}


// ── §1  ONE provider, MANY capabilities — the binary-negotiation fixture. ────
// Advertises HttpProvider (echoes `reply`), the full Capabilities lattice, AND
// claims Streaming. It does NOT implement Tools/Realtime — so a `match` over one
// value negotiates several capabilities with divergent verdicts (usage §1). Also
// the "structured_output = Yes -> send to inner" fixture for s36_Negotiated.
class s36_FullProvider {
    reply: string,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 full: no request offline"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 full: does not send offline"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["s36 full parse failed"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never { baml.ai.LegacyMeta {} }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s36 full parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }

    implements baml.ai.Capabilities {
        function structured_output(self) -> baml.ai.Support throws never { baml.ai.Support.Yes }
        function image_input(self) -> baml.ai.Support throws never { baml.ai.Support.Yes }
        function parallel_tools(self) -> baml.ai.Support throws never { baml.ai.Support.No }  // graded: tools yes, parallel no
        function max_input_tokens(self) -> int throws never { 128000 }
    }

    implements baml.ai.Streaming {
        function stream_messages<TStream, TFinal>(self, messages: baml.ai.ChatMessage[])
            -> baml.llm.Stream<TStream, TFinal>
            throws baml.errors.StreamError | baml.errors.UnknownError {
            // Streaming is CLAIMED (negotiation sees it); building a real Stream
            // offline needs the host accumulator/SSE (P8), so driving it throws.
            throw baml.errors.Unsupported { message: "s36 full: offline cannot construct a Stream (P8); capability is claimed" }
        }
    }
}


// ── §2  A provider whose static answer is `Maybe` — the gradient fixture. ────
// Its Capabilities says `structured_output = Maybe`: the static table can't decide,
// so the runtime probe is the truth. `will_reject` models a payload the provider
// turns out to choke on — the probe throws a typed `s36_SchemaRejected` so a
// negotiator can downgrade instead of failing.
class s36_MaybeProvider {
    reply: string,
    will_reject: bool,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 maybe: no request offline"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 maybe: does not send offline"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["s36 maybe parse failed"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never { baml.ai.LegacyMeta {} }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            // THE RUNTIME PROBE resolving the `Maybe`.
            if (self.will_reject) {
                throw s36_SchemaRejected { message: "s36 maybe: this payload's schema was rejected at runtime" };
            }
            let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["s36 maybe parse failed"] },
            };
            baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
        }
    }

    implements baml.ai.Capabilities {
        function structured_output(self) -> baml.ai.Support throws never { baml.ai.Support.Maybe }
        function image_input(self) -> baml.ai.Support throws never { baml.ai.Support.Maybe }
        function parallel_tools(self) -> baml.ai.Support throws never { baml.ai.Support.No }
        function max_input_tokens(self) -> int throws never { 32000 }
    }
}


// ── §3  A provider whose static answer is `No` — the skip-the-doomed-call fixture.
// structured_output = No: a negotiator skips the constrained call ENTIRELY (no
// wasted 400) and runs the degraded path. Its HttpProvider would throw if driven —
// proving `s36_Negotiated` never reaches it on a `No` verdict.
class s36_NoProvider {
    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 no: no request"] }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 no: does not send"] }
        }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 no: does not parse"] }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never { baml.ai.LegacyMeta {} }
        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            // If a negotiator honored the `No` verdict this is NEVER reached.
            throw s36_SchemaRejected { message: "s36 no: structured output statically unsupported — should have been skipped" };
        }
    }

    implements baml.ai.Capabilities {
        function structured_output(self) -> baml.ai.Support throws never { baml.ai.Support.No }
        function image_input(self) -> baml.ai.Support throws never { baml.ai.Support.Yes }
        function parallel_tools(self) -> baml.ai.Support throws never { baml.ai.Support.No }
        function max_input_tokens(self) -> int throws never { 8000 }
    }
}


// ── §4  A stream-ONLY provider — the call->stream degrade fixture. ───────────
// Implements Streaming but NOT HttpProvider. When the plain-value companion
// `drive_call` (behind `s36_Extract`) meets it, it degrades: `HttpProvider` absent,
// so it takes the `Streaming` arm and drains `.final()`. Building a real Stream
// offline is P8-blocked, so this throws a typed StreamError — which surfaces
// THROUGH drive_call's Streaming arm, proving the degrade ROUTING (the `.final()`
// value itself is the live tier). Contrast NoCapProvider (neither) -> Unsupported.
class s36_StreamOnly {
    implements baml.ai.Provider {}

    implements baml.ai.Streaming {
        function stream_messages<TStream, TFinal>(self, messages: baml.ai.ChatMessage[])
            -> baml.llm.Stream<TStream, TFinal>
            throws baml.errors.StreamError | baml.errors.UnknownError {
            // The typed StreamError proves drive_call routed into the Streaming arm
            // (a non-degrading provider would surface Unsupported instead).
            throw s36_StreamDegradeReached { message: "s36 stream-only: drive_call degraded call -> stream (offline cannot build a Stream; live proves .final())" }
        }
    }
}

// A typed StreamError marking that the call->stream degrade arm was reached.
class s36_StreamDegradeReached {
    message: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { false }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.StreamError {
        function is_network_error(self) -> bool { false }
    }
}


// ── §5  Route a Provider marker to its HttpProvider call path. ───────────────
// Members are the bare `Provider` marker (no call_messages_with), so the forward
// must MATCH to HttpProvider first — the projection is threaded intact.
function s36_call<T, V, E2>(
    p: baml.ai.Provider,
    messages: baml.ai.ChatMessage[],
    project: (baml.ai.ResponseMeta) -> V throws E2,
) -> baml.ai.CallResult<T, V>
    throws baml.errors.CallError | baml.errors.UnknownError | E2 {
    let picked = p;
    match (picked) {
        let h: baml.ai.HttpProvider => h.call_messages_with<T, V, E2>(messages, project),
        _ => throw baml.errors.Unsupported { message: "s36: provider has no HttpProvider call path" },
    }
}


// ── §6  The negotiation wrapper — the graded bridge (design's `Negotiated`). ──
// Reads inner's `Capabilities` lattice and routes:
//   Yes   -> send to inner (confident).
//   No    -> skip the doomed call, run the degraded path (no wasted 400).
//   Maybe -> PROBE inner; on a typed s36_SchemaRejected, fall back to degraded.
//   (no Capabilities surface) -> pure runtime-probe: try inner, degrade on rejection.
// A plain non-generic class that `implements HttpProvider` by overriding the
// primitive `call_messages_with` and routing to a member (the Fallback/Retry
// template). It logs a verdict at every branch (Aaron's observability rule).
class s36_Negotiated {
    inner: baml.ai.Provider,
    degraded: baml.ai.Provider,

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 negotiated: forwards calls; no single request"] }
        }
        function send(self, request: baml.http.Request) -> unknown throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 negotiated: does not send directly"] }
        }
        function parse<T>(self, from: unknown) -> T throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 negotiated: does not parse directly"] }
        }
        function parse_meta(self, from: unknown) -> baml.ai.ResponseMeta throws baml.errors.UnknownError {
            throw baml.errors.UnknownError { data: null, message: ["s36 negotiated: has no single meta"] }
        }

        function call_messages_with<T, V, E2>(
            self,
            messages: baml.ai.ChatMessage[],
            project: (baml.ai.ResponseMeta) -> V throws E2,
        ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
            match (self.inner) {
                let cap: baml.ai.Capabilities => match (cap.structured_output()) {
                    baml.ai.Support.Yes => {
                        log.info("36: verdict Yes -> send to inner");
                        s36_call<T, V, E2>(self.inner, messages, project)
                    },
                    baml.ai.Support.No => {
                        log.info("36: verdict No -> skip doomed call, run degraded");
                        s36_call<T, V, E2>(self.degraded, messages, project)
                    },
                    baml.ai.Support.Maybe => {
                        log.info("36: verdict Maybe -> probe inner at runtime");
                        s36_call<T, V, E2>(self.inner, messages, project) catch (e) {
                            // Triage by classifier: a schema rejection (is_parse_error)
                            // is a DOWNGRADE trigger; a transport error is NOT (don't
                            // degrade on a network blip). s36_SchemaRejected rides the
                            // CallError channel as its typed self (is_parse_error -> true).
                            let c: baml.errors.CallError => if (c.is_parse_error()) {
                                log.info("36: Maybe probe rejected at runtime (parse) -> degrade");
                                s36_call<T, V, E2>(self.degraded, messages, project)
                            } else {
                                throw e
                            },
                            _ => throw e,
                        }
                    },
                },
                _ => {
                    log.info("36: no Capabilities surface -> pure runtime probe");
                    s36_call<T, V, E2>(self.inner, messages, project) catch (e) {
                        let c: baml.errors.CallError => if (c.is_parse_error()) {
                            log.info("36: runtime schema rejection -> degrade");
                            s36_call<T, V, E2>(self.degraded, messages, project)
                        } else {
                            throw e
                        },
                        _ => throw e,
                    }
                },
            }
        }
    }
}
