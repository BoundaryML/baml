// scenario 07 — reasoning models: ../../../../../llm-provider/ideas/scenarios/07-reasoning/implement.baml
//   implementation.baml — the STD-LIB / library-author half.
//
// The design stresses the Provider/capability model against reasoning models. The
// two halves of reasoning split across the value+sidecar PARTITION:
//   - reasoning *text* (the displayable/billable view) ACCOMPANIES the answer, so
//     it is a PRODUCT sidecar → it rides `HttpProvider.call_with` (the real `$with`
//     companion) + a `ResponseMeta` projection (`s07_ReasoningMeta.reasoning_blocks()`).
//     The bespoke `Thinks.think -> WithReasoning<T>` companion the scenario once
//     invented is GONE — exactly the parallel-companion-per-sidecar mistake P1 retires.
//   - the continuity *handle* is a STATEFUL round-trip (resend signed/encrypted blocks,
//     or a server-held response id) → it stays its OWN capability (`s07_Continuity`),
//     because the hard part is *state* (P4), not the carrier shape. `CallResult<T,V>`
//     cannot express "thread this back into the next turn."
//
// ── Mapping the design onto the REAL surface (see REALIZED.md / _conventions.md) ──
//   - `ResponseMeta` / `Usage` / the `Supported<T> = T | Unsupported` type are all
//     stdlib: `baml.ai.ResponseMeta`, `baml.ai.Usage`, and `T | baml.ai.Unavailable`
//     (generic type aliases aren't a language feature, so the union is spelled inline).
//   - `(T, U)` tuples aren't a language feature → the value+sidecar carrier is
//     `baml.ai.CallResult<T, V>`; `.call_with` is the generated `Foo$with` companion.
//   - `baml.errors.ContinuityError` can't be added to the stdlib `baml.errors`
//     namespace from user code → it lives here as `s07_ContinuityError` (a scenario
//     interface, same shape + the net-new `is_cross_provider()` classifier).
//   - the design's real-HTTP `OpenAIReasoning`/`AnthropicReasoning` (which map to the
//     stdlib `baml.ai.OpenAiResponses` / `baml.ai.Anthropic` used in the live tier)
//     are hand-rolled OFFLINE here so the NET-NEW reasoning-content-kind + Continuity
//     surface is exercised deterministically with no network: they run the real codec
//     path (build_request/send/parse/parse_meta) over a scripted wire body.

// ---------------------------------------------------------------------------
// 1. Reasoning as a DISTINCT content kind (the load-bearing "what's hard").
// ---------------------------------------------------------------------------
// Reasoning is NOT text. We model it so it can be displayed, billed, stripped, or
// round-tripped without contaminating the answer `T`. The *body* of a reasoning
// block diverges wildly per provider, so `s07_ReasoningRaw` keeps the union of the
// three real wire shapes verbatim (product-with-optionals — unions don't cross the
// host boundary). `s07_ReasoningBlock` is the *displayable/billable* view;
// `s07_ContinuationState` (§4) is the *round-trippable* view — deliberately separate,
// because the encrypted OpenAI case is opaque even though it is continuity-critical.
class s07_ReasoningBlock {
    // What the user can actually read, if anything. Empty for encrypted-only.
    summary: string,
    // Token cost of the hidden deliberation, broken out from answer tokens.
    reasoning_tokens: int,
    // The raw provider-shaped payload, kept verbatim so nothing is lost.
    raw: s07_ReasoningRaw,
}

class s07_ReasoningRaw {
    // Anthropic extended-thinking: full text + signature (both REQUIRED on
    // round-trip when tools are involved — dropping/editing breaks the next turn).
    anthropic_thinking: string?,
    anthropic_signature: string?,
    // OpenAI Responses: opaque encrypted reasoning item you can't read but resend.
    openai_encrypted: string?,
    // Gemini: plain "thought" part, no signature, safe to drop.
    gemini_thought: string?,
}

// ---------------------------------------------------------------------------
// 2. The reasoning-TEXT carrier: a `ResponseMeta` dimension, NOT a wrapper.
// ---------------------------------------------------------------------------
// `s07_ReasoningMeta` is the response-metadata dimension for reasoning TEXT. It is
// an external-impl EXTENSION of `baml.ai.ResponseMeta` (the `Grounded` pattern in
// value-sidecar-model.md §7): `requires baml.ai.ResponseMeta` so a projector holding
// a `ResponseMeta` can reach it by runtime `match` (a sub-interface membership test —
// the capability-negotiation backbone). Not every response carries reasoning, so the
// projector falls back to `Unavailable` when the `match` misses.
//
// `reasoning_blocks()` is a NOVEL dimension, RICHER than the base
// `baml.ai.ResponseMeta.reasoning() -> string | Unavailable` summary. Lazy: a
// projector that only wants usage never pays to parse these.
interface s07_ReasoningMeta requires baml.ai.ResponseMeta {
    function reasoning_blocks(self) -> s07_ReasoningBlock[] | baml.ai.Unavailable throws never
}

// ---------------------------------------------------------------------------
// 3. The two incompatible control models, reconciled by NOT unifying them.
// ---------------------------------------------------------------------------
// `effort` is categorical (OpenAI). `thinking_budget` is a token count
// (Anthropic/Gemini). They are genuinely different knobs. We DO NOT invent a fake
// unified scalar — each provider declares the field(s) it actually honors as plain
// fields (the "options dissolve into fields" rule). A shared read-interface lets
// tooling reason about "is this a reasoning call" uniformly without forcing one knob.
enum s07_Effort { none, low, medium, high, xhigh }   // OpenAI gpt-5.5 rungs
enum s07_Verbosity { low, medium, high }             // independent answer-length knob

// Shared read-interface: a reasoning provider exposes whichever knob it honors. Both
// are optional because no provider honors both (mirrors ChatModelOptions in §4). The
// design spells these as interface FIELDS; the real surface expresses a read-interface
// as best-effort accessor METHODS (like `baml.ai.ResponseMeta`), so a provider can
// answer `null` for a knob it lacks without declaring a dead field.
interface s07_ReasoningOptions {
    function effort(self) -> s07_Effort? throws never
    function thinking_budget(self) -> int? throws never
    function verbosity(self) -> s07_Verbosity? throws never
}

// ---------------------------------------------------------------------------
// 4. Continuity: lossless cross-turn round-trip is its OWN capability.
// ---------------------------------------------------------------------------
// Two server-vs-client models collide:
//   - Anthropic / Gemini: continuity is CLIENT-side — you resend the prior reasoning
//     blocks (with signatures) in the next assistant turn.
//   - OpenAI Responses: continuity is SERVER-side — you pass a `previous_response_id`
//     and the server holds the reasoning; OR you resend the encrypted items yourself.
// `s07_ContinuationState` is an opaque, provider-owned union covering both (the
// reasoning analog of the `Tools.Transcript` opaque type). The app never inspects it.
class s07_ContinuationState {
    // OpenAI server-authoritative handle. When present, the next request sends
    // `previous_response_id` and omits resent reasoning entirely.
    openai_previous_response_id: string?,
    // Client-authoritative path: the verbatim reasoning blocks to resend. Carries
    // Anthropic signatures and OpenAI encrypted blobs untouched.
    resend_blocks: s07_ReasoningRaw[],
    // Which provider produced this — guards against cross-provider continuity, which
    // is lossy/illegal (Anthropic signed thinking has no OpenAI/Gemini equivalent).
    origin_provider: string,
}

// The response-metadata dimension for the continuity HANDLE (as opposed to the
// reasoning text). Extracting the handle is metadata that accompanies a turn, so a
// turn surfaces it via `$with` + this projection — but what you DO with it (thread it
// into the next turn) is the stateful `s07_Continuity` capability, NOT a sidecar.
interface s07_ContinuationMeta requires baml.ai.ResponseMeta {
    // The opaque continuity handle to feed into the NEXT turn's `resume`. Absent when
    // the provider has no cross-turn reasoning state (effort=none, plain chat).
    function continuation(self) -> s07_ContinuationState | baml.ai.Unavailable throws never
}

// ---------------------------------------------------------------------------
// NET-NEW capability-error channel: continuity (cross-turn reasoning resume). The
// design puts this at `baml.errors.ContinuityError`; user code can't extend the
// stdlib `baml.errors` namespace, so it lives here (same shape as the core CapErr
// interfaces + one classifier the call path has no notion of: `is_cross_provider`,
// the runtime origin-mismatch guard `resume` must enforce — Anthropic state on OpenAI).
// ---------------------------------------------------------------------------
interface s07_ContinuityError {
    function is_network_error(self) -> bool
    function is_rate_limit(self) -> bool
    function is_parse_error(self) -> bool
    function is_cross_provider(self) -> bool
}

// A resume() against state from a different provider: lossless continuity is
// impossible, so this is a deliberate, typed domain failure — NOT a transport error.
class s07_CrossProviderContinuity {
    expected_provider: string,
    state_provider: string,

    implements s07_ContinuityError {
        function is_network_error(self) -> bool { false }
        function is_rate_limit(self) -> bool { false }
        function is_parse_error(self) -> bool { false }
        function is_cross_provider(self) -> bool { true }
    }
}

// The Continuity capability: not every provider can resume reasoning. A provider that
// implements it promises a lossless `resume` that injects prior state into the next
// prompt's request. This is the STATEFUL half of reasoning — deliberately a capability
// and NOT a `$with` sidecar, because `CallResult<T,V>` cannot carry server/client
// state across turns (P4). It is an ordinary capability negotiated by a runtime `match`
// (the design's §5 pattern) — `.resume` honestly errors on a non-Continuity provider.
//
// NOTE: this is deliberately NOT a `//baml:llm_capability` + `//baml:llm_companion`
// pair (the ns_ai_custom_capability recipe). A generated `Foo$resume` companion is
// materialized on EVERY LLM function corpus-wide, and its `state: s07_ContinuationState`
// param — a user class in THIS namespace — fails to resolve for functions in other
// namespaces (the cross-namespace-signature codegen gap in _plan/baml_gotchas.md),
// crashing runtime lowering. The design itself resumes via a direct `match (p) { let c:
// s07_Continuity => c.resume(...) }`, so a plain capability interface is the faithful
// AND working choice; the moderated companion (string param) shows the companion path.
interface s07_Continuity requires baml.ai.Provider {
    // Fold a prior turn's reasoning state into the prompt and return just the answer
    // `T`. Throws `s07_CrossProviderContinuity` if `state.origin_provider` doesn't
    // match this provider (no cross-provider reasoning continuity — genuinely lossy).
    function resume<T>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState) -> T
        throws s07_ContinuityError | baml.errors.UnknownError

    // The value+sidecar form of `resume` (the design's `resume_with`): returns the
    // answer AND a projection of the resulting ResponseMeta (reasoning text + the NEXT
    // continuation handle), so a multi-turn loop threads state forward without a
    // bespoke wrapper. Same partition, one turn later.
    function resume_with<T, V, E2>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState,
        project: (baml.ai.ResponseMeta) -> V throws E2) -> baml.ai.CallResult<T, V>
        throws s07_ContinuityError | baml.errors.UnknownError | E2
}

// The negotiation helper for `resume`: match the client's provider for the
// `s07_Continuity` capability (the §5 runtime `match`), else honestly error. This is
// what a caller writes inline in the design's usage.baml; factored here so the two
// call sites share it. NOTE: there is NO `Thinks` capability — "does this provider
// produce reasoning?" is answered by whether its ResponseMeta implements
// `s07_ReasoningMeta`, discovered by the same runtime `match` the projector does.
function s07_resume_on<T>(client: baml.ai.Provider, followup: baml.llm.PromptAst, state: s07_ContinuationState) -> T
    throws baml.errors.Unsupported | s07_ContinuityError | baml.errors.UnknownError {
    let p = client;
    match (p) {
        let c: s07_Continuity => c.resume<T>(baml.ai.prompt_to_messages(followup), state),
        _ => throw baml.errors.Unsupported { message: "client's provider is not s07_Continuity" },
    }
}

// ---------------------------------------------------------------------------
// 5. OpenAI reasoning provider (Responses API shape): effort + verbosity + encrypted
//    continuity + SERVER-side previous_response_id.
//    Real-surface counterpart: baml.ai.OpenAiResponses (live tier). Hand-rolled
//    offline here to carry the net-new reasoning-content-kind + Continuity surface.
// ---------------------------------------------------------------------------
// The concrete ResponseMeta the OpenAI reasoning provider normalizes lazily. It
// implements the base `baml.ai.ResponseMeta` PLUS the two external-impl reasoning
// dimensions (`s07_ReasoningMeta`, `s07_ContinuationMeta`). A projector reaches the
// reasoning dimensions by `match (m) { let rm: s07_ReasoningMeta => ... }`.
class s07_OpenAiReasoningMeta {
    // scripted wire fields (offline): what a real Responses body would carry.
    summary_text: string,           // reasoning summary (human-readable)
    reasoning_tokens: int,          // usage.output_tokens_details.reasoning_tokens
    input_tokens: int,
    output_tokens: int,
    encrypted: string,              // opaque encrypted reasoning item (client-held path)
    response_id: string,            // server-held handle (previous_response_id path)
    include_encrypted: bool,        // decides server-id vs client-resend continuation

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "completed" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.input_tokens, output_tokens: self.output_tokens }
        }
        // The base `reasoning()` is the plain-text summary view; the richer blocks
        // ride `s07_ReasoningMeta` below.
        function reasoning(self) -> string | baml.ai.Unavailable throws never { self.summary_text }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai responses" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "openai responses" }
        }
    }
    implements s07_ReasoningMeta {
        function reasoning_blocks(self) -> s07_ReasoningBlock[] | baml.ai.Unavailable throws never {
            [s07_ReasoningBlock {
                summary: self.summary_text,
                reasoning_tokens: self.reasoning_tokens,
                raw: s07_ReasoningRaw {
                    anthropic_thinking: null, anthropic_signature: null,
                    openai_encrypted: self.encrypted, gemini_thought: null,
                },
            }]
        }
    }
    implements s07_ContinuationMeta {
        function continuation(self) -> s07_ContinuationState | baml.ai.Unavailable throws never {
            if (self.include_encrypted) {
                // CLIENT-authoritative: we hold the encrypted reasoning items.
                s07_ContinuationState {
                    openai_previous_response_id: null,
                    resend_blocks: [s07_ReasoningRaw {
                        anthropic_thinking: null, anthropic_signature: null,
                        openai_encrypted: self.encrypted, gemini_thought: null,
                    }],
                    origin_provider: "openai",
                }
            } else {
                // SERVER-authoritative: the server holds the reasoning by id.
                s07_ContinuationState {
                    openai_previous_response_id: self.response_id,
                    resend_blocks: [],
                    origin_provider: "openai",
                }
            }
        }
    }
}

class s07_OpenAiReasoning {
    model: string,                  // "gpt-5.5", "o4-mini"
    api_key: string,
    effort: s07_Effort,             // none | low | medium | high | xhigh — categorical knob
    verbosity: s07_Verbosity,       // independent answer-length knob
    summary: string,                // "auto" | "concise" | "detailed" | "none"
    // When true we ask the server to return encrypted reasoning items so WE hold
    // continuity (stateless). When false, continuity is server-side via response id.
    include_encrypted: bool,
    // --- offline scripting (a real provider gets these off the wire) ---
    reply: string,                  // the JSON answer body SAP-parsed into T
    scripted_summary: string,       // the reasoning summary text this call "produced"
    scripted_reasoning_tokens: int,

    // effort is honored; OpenAI has no token budget (the read-interface answers null).
    implements s07_ReasoningOptions {
        function effort(self) -> s07_Effort? throws never { self.effort }
        function thinking_budget(self) -> int? throws never { null }
        function verbosity(self) -> s07_Verbosity? throws never { self.verbosity }
    }

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        // The DIVERGENCE lives here: effort/verbosity/summary go into the Responses
        // `reasoning{}` and `text{}` blocks — a shape no other provider shares.
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            baml.http.Request { method: "POST", url: "https://example.invalid/responses", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError { self.reply }
        // Plain `call<T>` DISCARDS reasoning — answer only. This is what
        // `function Foo(args) -> T` gets for free.
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["openai reasoning parse"] } }
        }
        // The value+sidecar seam: normalize the wire response into a ResponseMeta
        // (lazy). `$with` derives from one codec pass and projects this; reasoning
        // surfaces HERE, not via a `think` companion — and because `call_with` is an
        // HttpProvider capability, Fallback/Retry forward it instead of dropping it.
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s07_OpenAiReasoningMeta {
                summary_text: self.scripted_summary,
                reasoning_tokens: self.scripted_reasoning_tokens,
                input_tokens: 40, output_tokens: 12,
                encrypted: "enc:" + self.scripted_summary,
                response_id: "resp_openai_1",
                include_encrypted: self.include_encrypted,
            }
        }
    }

    implements s07_Continuity {
        // `resume<T> -> T`: the answer only. Reasoning text + the next continuation
        // handle are PRODUCT sidecars projected via `resume_with` below.
        function resume<T>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState) -> T
            throws s07_ContinuityError | baml.errors.UnknownError {
            let _r = s07_OpenAiReasoning.resume_guard(self, state);
            baml.sap.parse<T>(self.reply) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["openai resume parse"] } }
        }
        function resume_with<T, V, E2>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState,
            project: (baml.ai.ResponseMeta) -> V throws E2) -> baml.ai.CallResult<T, V>
            throws s07_ContinuityError | baml.errors.UnknownError | E2 {
            let _r = s07_OpenAiReasoning.resume_guard(self, state);
            let v: T = baml.sap.parse<T>(self.reply) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["openai resume_with parse"] } };
            let meta: baml.ai.ResponseMeta = s07_OpenAiReasoningMeta {
                summary_text: self.scripted_summary, reasoning_tokens: self.scripted_reasoning_tokens,
                input_tokens: 40, output_tokens: 12, encrypted: "enc:" + self.scripted_summary,
                response_id: "resp_openai_2", include_encrypted: self.include_encrypted,
            };
            baml.ai.CallResult<T, V> { value: v, meta: project(meta) }
        }
    }

    // The origin guard shared by resume / resume_with. Server-id vs client-resend is
    // decided by which state arm is populated (a real provider branches the request);
    // offline both paths land on the same scripted reply.
    function resume_guard(self, state: s07_ContinuationState) -> bool throws s07_ContinuityError {
        if (state.origin_provider != "openai") {
            throw s07_CrossProviderContinuity { expected_provider: "openai", state_provider: state.origin_provider };
        }
        true
    }
}

// ---------------------------------------------------------------------------
// 6. Anthropic extended-thinking provider: TOKEN budget, signed thinking blocks,
//    CLIENT-side continuity (must resend signatures).
//    Real-surface counterpart: baml.ai.Anthropic (live tier).
// ---------------------------------------------------------------------------
class s07_AnthropicReasoningMeta {
    thinking_text: string,          // Anthropic exposes the full thinking text
    signature: string,              // cryptographic signature (required on round-trip)
    reasoning_tokens: int,
    input_tokens: int,
    output_tokens: int,

    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "end_turn" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.input_tokens, output_tokens: self.output_tokens }
        }
        // Anthropic exposes the full thinking text — the base summary view IS it.
        function reasoning(self) -> string | baml.ai.Unavailable throws never { self.thinking_text }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic: none" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "anthropic: none" }
        }
    }
    implements s07_ReasoningMeta {
        function reasoning_blocks(self) -> s07_ReasoningBlock[] | baml.ai.Unavailable throws never {
            [s07_ReasoningBlock {
                summary: self.thinking_text,
                reasoning_tokens: self.reasoning_tokens,
                raw: s07_ReasoningRaw {
                    anthropic_thinking: self.thinking_text, anthropic_signature: self.signature,
                    openai_encrypted: null, gemini_thought: null,
                },
            }]
        }
    }
    implements s07_ContinuationMeta {
        function continuation(self) -> s07_ContinuationState | baml.ai.Unavailable throws never {
            // Continuity is the verbatim signed thinking blocks — client holds them.
            s07_ContinuationState {
                openai_previous_response_id: null,
                resend_blocks: [s07_ReasoningRaw {
                    anthropic_thinking: self.thinking_text, anthropic_signature: self.signature,
                    openai_encrypted: null, gemini_thought: null,
                }],
                origin_provider: "anthropic",
            }
        }
    }
}

class s07_AnthropicReasoning {
    model: string,                  // "claude-sonnet-4-5"
    api_key: string,
    max_tokens: int,                // REQUIRED on Anthropic; MUST exceed the budget
    thinking_budget: int,           // token budget — the OTHER control model
    // No `effort`, no `verbosity`: Anthropic exposes neither. Answer length is left to
    // the prompt. This is the "two-knob model doesn't map cleanly" leak (evaluation (c)).
    // --- offline scripting ---
    reply: string,
    scripted_thinking: string,
    scripted_reasoning_tokens: int,

    implements s07_ReasoningOptions {
        function effort(self) -> s07_Effort? throws never { null }              // no categorical effort
        function thinking_budget(self) -> int? throws never { self.thinking_budget }
        function verbosity(self) -> s07_Verbosity? throws never { null }        // no verbosity knob
    }

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            // DIVERGENCE: thinking goes in `thinking: { type: "enabled",
            // budget_tokens: <thinking_budget> }`, and `max_tokens` MUST exceed the
            // budget or the visible answer truncates to nothing (evaluation OQ #4).
            baml.http.Request { method: "POST", url: "https://example.invalid/messages", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError { self.reply }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            // A real provider walks content[] for the `text` block (skipping `thinking`
            // blocks) and SAP-parses it; offline the reply IS the answer text.
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["anthropic reasoning parse"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s07_AnthropicReasoningMeta {
                thinking_text: self.scripted_thinking,
                signature: "sig:" + self.scripted_thinking,
                reasoning_tokens: self.scripted_reasoning_tokens,
                input_tokens: 55, output_tokens: 18,
            }
        }
    }

    implements s07_Continuity {
        function resume<T>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState) -> T
            throws s07_ContinuityError | baml.errors.UnknownError {
            let _r = s07_AnthropicReasoning.resume_guard(self, state);
            baml.sap.parse<T>(self.reply) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["anthropic resume parse"] } }
        }
        function resume_with<T, V, E2>(self, messages: baml.ai.ChatMessage[], state: s07_ContinuationState,
            project: (baml.ai.ResponseMeta) -> V throws E2) -> baml.ai.CallResult<T, V>
            throws s07_ContinuityError | baml.errors.UnknownError | E2 {
            let _r = s07_AnthropicReasoning.resume_guard(self, state);
            let v: T = baml.sap.parse<T>(self.reply) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["anthropic resume_with parse"] } };
            let meta: baml.ai.ResponseMeta = s07_AnthropicReasoningMeta {
                thinking_text: self.scripted_thinking, signature: "sig:" + self.scripted_thinking,
                reasoning_tokens: self.scripted_reasoning_tokens, input_tokens: 55, output_tokens: 18,
            };
            baml.ai.CallResult<T, V> { value: v, meta: project(meta) }
        }
    }

    // CLIENT-authoritative ONLY: a real resume re-injects the signed thinking blocks
    // verbatim into the assistant turn (editing/dropping a signature breaks the turn).
    function resume_guard(self, state: s07_ContinuationState) -> bool throws s07_ContinuityError {
        if (state.origin_provider != "anthropic") {
            throw s07_CrossProviderContinuity { expected_provider: "anthropic", state_provider: state.origin_provider };
        }
        true
    }
}

// ---------------------------------------------------------------------------
// 7. A "reasoning-naive" provider for portability tests: plain chat has NO thinking.
//    It implements Provider/HttpProvider, but its ResponseMeta does NOT implement
//    s07_ReasoningMeta / s07_ContinuationMeta, and it does NOT implement s07_Continuity.
//    Used to show where the reasoning projection lands on `Unavailable` (via a `_` arm)
//    and where `.resume` honestly errors (the capability is simply absent).
// ---------------------------------------------------------------------------
class s07_PlainChatMeta {
    input_tokens: int,
    output_tokens: int,
    implements baml.ai.ResponseMeta {
        function finish_reason(self) -> string throws never { "stop" }
        function usage(self) -> baml.ai.Usage throws never {
            baml.ai.Usage { input_tokens: self.input_tokens, output_tokens: self.output_tokens }
        }
        function reasoning(self) -> string | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "non-reasoning model" }
        }
        function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "plain chat" }
        }
        function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
            baml.ai.Unavailable { reason: "plain chat" }
        }
    }
    // NO s07_ReasoningMeta / s07_ContinuationMeta impls — so a reasoning projection's
    // `match` misses and lands on the projector's `_` arm (`Unavailable`). This is the
    // value+sidecar form of "this provider literally cannot think."
}

class s07_PlainChat {
    model: string,
    api_key: string,
    reply: string,                  // offline scripting

    implements baml.ai.Provider {}

    implements baml.ai.HttpProvider {
        type Body = string
        function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
            baml.http.Request { method: "POST", url: "https://example.invalid/chat", headers: {}, body: "" }
        }
        function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError { self.reply }
        function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
            baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["plainchat parse"] } }
        }
        function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
            s07_PlainChatMeta { input_tokens: 20, output_tokens: 8 }
        }
    }
    // No `implements s07_Continuity`, and its ResponseMeta carries no reasoning
    // dimension. The capability set IS the type: this provider literally cannot think.
}
