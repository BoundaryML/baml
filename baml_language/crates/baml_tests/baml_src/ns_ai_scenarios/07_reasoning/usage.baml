// scenario 07 — reasoning models: ../../../../../llm-provider/ideas/scenarios/07-reasoning/usage.baml
//   usage.baml — APPLICATION-author code. Uses implementation.baml.
//
// Shows, on the REAL surface: how little the app writes for the common case; how
// reasoning *text* is pulled out-of-band with the `$with` companion + a `ResponseMeta`
// projection (NOT a bespoke `.think` companion); how the two control models look
// side-by-side (categorical effort vs token budget); how the continuity HANDLE rides
// the same projection but is threaded back through the `s07_Continuity.resume`
// capability (a direct runtime `match`, via `s07_resume_on`); and where portability leaks
// (s07_PlainChat's response carries no s07_ReasoningMeta → a typed Unavailable).
//
// Real-surface substitutions vs the design (see implementation.baml header):
//   - `.call_with(args, proj)` → `Foo$with(args, proj, client = P)`  (a CallResult<T,V>)
//   - `(T, U)` tuple           → `baml.ai.CallResult<T, V>`
//   - `Supported<T>`           → `T | baml.ai.Unavailable`
//   - `.resume(...)`           → `Foo$resume(args, state, client = P)` (custom capability)
//   - offline providers are swapped in via a `client =` override; the default `client`
//     is a dead one so every offline test must pass an explicit deterministic provider.

class s07_Diagnosis {
    root_cause: string,
    fix: string,
    confidence: float,
}

// Dead default client — every offline test overrides it with `client = <provider>`.
client<llm> DeadS07 {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

// ---------------------------------------------------------------------------
// 1. The common case: a normal `-> T` function. Effort/verbosity/summary are just
//    provider fields; reasoning is DISCARDED on the plain call (`.call<T>`), exactly
//    like today. One prompt in, s07_Diagnosis out — no mention of reasoning at all.
// ---------------------------------------------------------------------------
function s07_Diagnose(build_log: string) -> s07_Diagnosis {
    client DeadS07
    prompt #"
        {{ _.role("user") }}
        Diagnose why this build is flaky:
        {{ build_log }}
    "#
}

// A DeepThinker (OpenAI effort model) is swapped in; reasoning is thrown away — the 90% path.
test "07: the plain call routes through a reasoning provider, discarding reasoning" {
    let r = s07_Diagnose(
        "flaky test on CI",
        client = s07_OpenAiReasoning {
            model: "gpt-5.5", api_key: "unused",
            effort: s07_Effort.high, verbosity: s07_Verbosity.low, summary: "auto",
            include_encrypted: false,
            reply: "{\"root_cause\":\"race in setup\",\"fix\":\"add a lock\",\"confidence\":0.8}",
            scripted_summary: "considered the ordering of fixtures",
            scripted_reasoning_tokens: 120,
        },
    ) catch (e) {
        _ => s07_Diagnosis { root_cause: "call failed", fix: "", confidence: 0.0 },
    };
    assert.equal(r.root_cause, "race in setup")
}

// ---------------------------------------------------------------------------
// 2. The OTHER control model: Anthropic uses a TOKEN budget, not an effort category.
//    Same s07_Diagnose body; the app just wires a different provider (which also sets
//    max_tokens > thinking_budget — an Anthropic constraint). This is the portability
//    claim AND its leak: the body is identical, but the knobs are not interchangeable.
// ---------------------------------------------------------------------------
test "07: the same body runs on a token-budget provider (the other control model)" {
    let r = s07_Diagnose(
        "flaky test on CI",
        client = s07_AnthropicReasoning {
            model: "claude-sonnet-4-5", api_key: "unused",
            max_tokens: 8192, thinking_budget: 4096,
            reply: "{\"root_cause\":\"clock skew\",\"fix\":\"pin the clock\",\"confidence\":0.7}",
            scripted_thinking: "the timestamps disagree across nodes",
            scripted_reasoning_tokens: 300,
        },
    ) catch (e) {
        _ => s07_Diagnosis { root_cause: "call failed", fix: "", confidence: 0.0 },
    };
    assert.equal(r.root_cause, "clock skew")
}

// The two control models observed uniformly via the shared read-interface, without
// knowing the concrete provider (design §a "shared read-interface"): OpenAI honors
// `effort` (budget null); Anthropic honors `thinking_budget` (effort/verbosity null).
function s07_describe_knobs(o: s07_ReasoningOptions) -> string throws never {
    let eff = match (o.effort()) { null => "none", let e: s07_Effort => "effort" };
    let bud = match (o.thinking_budget()) { null => "none", let b: int => "budget=" + b.to_string() };
    eff + "/" + bud
}

test "07: effort vs token budget are two fields on two classes, read uniformly" {
    let openai = s07_describe_knobs(s07_OpenAiReasoning {
        model: "gpt-5.5", api_key: "u", effort: s07_Effort.medium, verbosity: s07_Verbosity.high,
        summary: "auto", include_encrypted: false, reply: "{}", scripted_summary: "", scripted_reasoning_tokens: 0,
    });
    let anthropic = s07_describe_knobs(s07_AnthropicReasoning {
        model: "claude-sonnet-4-5", api_key: "u", max_tokens: 8192, thinking_budget: 4096,
        reply: "{}", scripted_thinking: "", scripted_reasoning_tokens: 0,
    });
    assert.equal(openai, "effort/none")
    assert.equal(anthropic, "none/budget=4096")
}

// ---------------------------------------------------------------------------
// 3. Surfacing reasoning *text* out-of-band: the `$with` companion + a ResponseMeta
//    projection. The app gets the answer AND the displayable/billable reasoning blocks
//    as a CallResult — without the answer type s07_Diagnosis being polluted by
//    reasoning fields, and without a bespoke `.think -> WithReasoning<T>` companion.
//    The reasoning dimension is NOVEL, so the projector reaches it by `match` and
//    falls back to `Unavailable` if the provider's response has no s07_ReasoningMeta.
// ---------------------------------------------------------------------------
test "07: reasoning text rides the $with sidecar as a ResponseMeta projection" {
    let r = s07_Diagnose$with(
        "flaky test on CI",
        (m: baml.ai.ResponseMeta) -> s07_ReasoningBlock[] | baml.ai.Unavailable {
            match (m) {
                let rm: s07_ReasoningMeta => rm.reasoning_blocks(),
                _                         => baml.ai.Unavailable { reason: "no reasoning meta on this response" },
            }
        },
        client = s07_OpenAiReasoning {
            model: "gpt-5.5", api_key: "unused",
            effort: s07_Effort.high, verbosity: s07_Verbosity.low, summary: "auto", include_encrypted: false,
            reply: "{\"root_cause\":\"race in setup\",\"fix\":\"add a lock\",\"confidence\":0.8}",
            scripted_summary: "considered the ordering of fixtures",
            scripted_reasoning_tokens: 120,
        },
    ) catch (e) {
        _ => baml.ai.CallResult<s07_Diagnosis, s07_ReasoningBlock[] | baml.ai.Unavailable> {
            value: s07_Diagnosis { root_cause: "with failed", fix: "", confidence: 0.0 },
            meta: baml.ai.Unavailable { reason: "err" },
        },
    };
    // The answer is intact AND separate from the reasoning.
    assert.equal(r.value.root_cause, "race in setup")
    let total = match (r.meta) {
        let blocks: s07_ReasoningBlock[] => {
            let t = 0;
            for (let b in blocks) { t = t + b.reasoning_tokens; }
            t
        },
        let u: baml.ai.Unavailable => -1,
    };
    assert.equal(total, 120)
}

// ---------------------------------------------------------------------------
// 5 (design order): Portability leak, shown on purpose. s07_PlainChat's response
//    carries NO s07_ReasoningMeta. The degradation is UNIFORM — the SAME `$with`
//    projection runs against every provider; PlainChat's response simply misses the
//    `s07_ReasoningMeta` match and lands on the projector's `_` arm (`Unavailable`).
//    No companion-vs-call branching, no silent empty wrapper — a TYPED "can't."
// ---------------------------------------------------------------------------
test "07: a non-reasoning provider misses the ReasoningMeta match — typed Unavailable" {
    let r = s07_Diagnose$with(
        "flaky test on CI",
        (m: baml.ai.ResponseMeta) -> s07_ReasoningBlock[] | baml.ai.Unavailable {
            match (m) {
                let rm: s07_ReasoningMeta => rm.reasoning_blocks(),
                _                         => baml.ai.Unavailable { reason: "non-reasoning model" },
            }
        },
        client = s07_PlainChat {
            model: "gpt-4o-mini", api_key: "unused",
            reply: "{\"root_cause\":\"disk full\",\"fix\":\"clean cache\",\"confidence\":0.6}",
        },
    ) catch (e) {
        _ => baml.ai.CallResult<s07_Diagnosis, s07_ReasoningBlock[] | baml.ai.Unavailable> {
            value: s07_Diagnosis { root_cause: "with failed", fix: "", confidence: 0.0 },
            meta: baml.ai.Unavailable { reason: "err" },
        },
    };
    assert.equal(r.value.root_cause, "disk full")   // answer still works
    let msg = match (r.meta) {
        let blocks: s07_ReasoningBlock[] => "got " + blocks.length().to_string() + " blocks",
        let u: baml.ai.Unavailable => "unavailable: " + u.reason,
    };
    assert.equal(msg, "unavailable: non-reasoning model")
}

// The base `reasoning()` dimension is the plain-text summary view (present on the
// reasoning provider, Unavailable on plain chat) — the same dimension, one rung down.
test "07: the base reasoning() summary is present on a thinker, Unavailable on plain chat" {
    let thinker = s07_Diagnose$with(
        "x",
        (m: baml.ai.ResponseMeta) -> string { match (m.reasoning()) { let s: string => s, let u: baml.ai.Unavailable => "unavailable" } },
        client = s07_OpenAiReasoning {
            model: "gpt-5.5", api_key: "u", effort: s07_Effort.low, verbosity: s07_Verbosity.low,
            summary: "auto", include_encrypted: false, reply: "{\"root_cause\":\"a\",\"fix\":\"b\",\"confidence\":0.5}",
            scripted_summary: "a short thought", scripted_reasoning_tokens: 10,
        },
    ) catch (e) { _ => baml.ai.CallResult<s07_Diagnosis, string> { value: s07_Diagnosis { root_cause: "", fix: "", confidence: 0.0 }, meta: "err" } };
    let plain = s07_Diagnose$with(
        "x",
        (m: baml.ai.ResponseMeta) -> string { match (m.reasoning()) { let s: string => s, let u: baml.ai.Unavailable => "unavailable" } },
        client = s07_PlainChat { model: "gpt-4o-mini", api_key: "u", reply: "{\"root_cause\":\"a\",\"fix\":\"b\",\"confidence\":0.5}" },
    ) catch (e) { _ => baml.ai.CallResult<s07_Diagnosis, string> { value: s07_Diagnosis { root_cause: "", fix: "", confidence: 0.0 }, meta: "err" } };
    assert.equal(thinker.meta, "a short thought")
    assert.equal(plain.meta, "unavailable")
}

// ---------------------------------------------------------------------------
// 4 (design order): Continuity across turns — the cross-turn "what's hard." The
//    PARTITION is visible: extracting the continuation HANDLE is a product sidecar
//    (it accompanies turn 1's answer), so it rides `$with` + a s07_ContinuationMeta
//    projection. But THREADING it back is a stateful round-trip, so turn 2 negotiates
//    the `s07_Continuity` capability via a direct `match` (the helper `s07_resume_on`,
//    which is exactly the design's `match (p) { let c: Continuity => c.resume(...) }`) —
//    CallResult<T,V> cannot express "carry this state into the next turn." The app
//    NEVER inspects the handle. The match negotiates at runtime; a provider that can't
//    resume yields a typed baml.errors.Unsupported.
// ---------------------------------------------------------------------------
// Spans TWO capability channels: turn 1 rides `$with` (the CallError path, plus the
// real desugar's render/transport machinery errors) and turn 2 rides `resume`
// (ContinuityError). The honest channel is their union plus the universal wrapper.
function s07_MultiTurnDiagnose(p: baml.ai.Provider, first_log: string, followup: string) -> s07_Diagnosis
    throws baml.errors.CallError | s07_ContinuityError | baml.errors.Unsupported
        | baml.errors.InvalidArgument | baml.errors.LlmClient | baml.errors.RenderPrompt | baml.errors.UnknownError {
    // Turn 1: get the answer AND the opaque continuity handle, projected off the
    // normalized ResponseMeta. No `.think`, no WithReasoning<T> carrier.
    let turn1 = s07_Diagnose$with(
        first_log,
        (m: baml.ai.ResponseMeta) -> s07_ContinuationState | baml.ai.Unavailable {
            match (m) {
                let cm: s07_ContinuationMeta => cm.continuation(),
                _                            => baml.ai.Unavailable { reason: "no continuity on this response" },
            }
        },
        client = p,
    );

    match (turn1.meta) {
        // Turn 2: feed the prior reasoning back in, losslessly — the STATEFUL half, on
        // the Continuity capability (NOT a sidecar). We only need the answer here.
        let state: s07_ContinuationState => s07_resume_on<s07_Diagnosis>(p, s07_Diagnose$render_prompt(followup), state),
        // No continuity state (effort=none / non-reasoning model): fall back to turn 1.
        let u: baml.ai.Unavailable => turn1.value,
    }
}

test "07: multi-turn — handle rides $with (sidecar), threaded back via resume (capability)" {
    let p: baml.ai.Provider = s07_OpenAiReasoning {
        model: "gpt-5.5", api_key: "unused",
        effort: s07_Effort.high, verbosity: s07_Verbosity.medium, summary: "detailed",
        include_encrypted: true,        // client-side continuity: WE hold the encrypted state
        reply: "{\"root_cause\":\"resumed root cause\",\"fix\":\"apply fix\",\"confidence\":0.9}",
        scripted_summary: "carried the earlier reasoning forward",
        scripted_reasoning_tokens: 200,
    };
    let r = s07_MultiTurnDiagnose(p, "first turn log", "and now?") catch (e) {
        _ => s07_Diagnosis { root_cause: "multiturn failed", fix: "", confidence: 0.0 },
    };
    assert.equal(r.root_cause, "resumed root cause")
}

// A provider WITHOUT the Continuity capability makes `resume` honestly error — the
// capability set is the type; there is no honest way to fake reasoning continuity.
test "07: resume on a non-Continuity provider is a typed Unsupported" {
    let plain: baml.ai.Provider = s07_PlainChat { model: "gpt-4o-mini", api_key: "u", reply: "{}" };
    // The catch arms must unify with the try-value type (s07_Diagnosis), so the outcome
    // is encoded in root_cause rather than returned as a bare string.
    let r = s07_resume_on<s07_Diagnosis>(
        plain,
        s07_Diagnose$render_prompt("followup"),
        s07_ContinuationState { openai_previous_response_id: null, resend_blocks: [], origin_provider: "openai" },
    ) catch (e) {
        let u: baml.errors.Unsupported => s07_Diagnosis { root_cause: "unsupported", fix: "", confidence: 0.0 },
        _ => s07_Diagnosis { root_cause: "wrong error", fix: "", confidence: 0.0 },
    };
    assert.equal(r.root_cause, "unsupported")
}

// The value+sidecar form of continuity: `resume_with` returns the answer AND the
// resulting ResponseMeta projection (reasoning text + the NEXT handle) in ONE turn.
// Exercised via a direct capability `match` (the compile-time-known concrete path).
test "07: resume_with threads state AND projects the next turn's reasoning" {
    let p: baml.ai.Provider = s07_AnthropicReasoning {
        model: "claude-sonnet-4-5", api_key: "unused", max_tokens: 8192, thinking_budget: 4096,
        reply: "{\"root_cause\":\"deeper cause\",\"fix\":\"structural fix\",\"confidence\":0.85}",
        scripted_thinking: "reconsidered with the prior thinking in context",
        scripted_reasoning_tokens: 250,
    };
    let state = s07_ContinuationState { openai_previous_response_id: null, resend_blocks: [], origin_provider: "anthropic" };
    let r = match (p) {
        let c: s07_Continuity =>
            c.resume_with<s07_Diagnosis, string, never>(
                baml.ai.prompt_to_messages(s07_Diagnose$render_prompt("continue")),
                state,
                (m: baml.ai.ResponseMeta) -> string { match (m.reasoning()) { let s: string => s, let u: baml.ai.Unavailable => "u" } },
            ) catch (e) { _ => baml.ai.CallResult<s07_Diagnosis, string> { value: s07_Diagnosis { root_cause: "resume_with failed", fix: "", confidence: 0.0 }, meta: "" } },
        _ => baml.ai.CallResult<s07_Diagnosis, string> { value: s07_Diagnosis { root_cause: "no capability", fix: "", confidence: 0.0 }, meta: "" },
    };
    assert.equal(r.value.root_cause, "deeper cause")
    assert.equal(r.meta, "reconsidered with the prior thinking in context")
}

// ---------------------------------------------------------------------------
// 8 (design order): Catching across the two channels, most-specific first. The
//    cross-provider continuity failure (the Fallback leak, evaluation (c)/(d)) is a
//    CONCRETE, catchable error now — not an opaque message string. This is also the
//    exact leak the design warns about: a handle projected from an OpenAI turn 1 can
//    NOT be resumed on an Anthropic member (origin_provider guard throws).
// ---------------------------------------------------------------------------
function s07_MultiTurnSafe(p: baml.ai.Provider, first_log: string, followup: string) -> string throws never {
    // `expr catch { arms }` must unify with the try-value's type; the arms use `return`
    // to exit the function with a string, leaving `d` bound to the answer on success.
    let d = s07_MultiTurnDiagnose(p, first_log, followup) catch (e) {
        // Concrete first: the origin-mismatch guard fired (resumed foreign state).
        let x: s07_CrossProviderContinuity =>
            return "[cannot resume " + x.state_provider + " state on " + x.expected_provider + "]",
        // Interface: any other continuity failure — branch on the new classifier.
        let c: s07_ContinuityError =>
            return if (c.is_cross_provider()) { "[cross-provider continuity]" }
                   else if (c.is_network_error()) { "[continuity network error — retry]" }
                   else { "[continuity failed]" },
        // Interface: a turn-1 (call-path) failure, triaged without the concrete class.
        let call_err: baml.errors.CallError =>
            return if (call_err.is_rate_limit()) { "[rate limited]" } else { "[call failed]" },
        // Escape hatch: anything foreign (render/transport machinery, boxed unknowns).
        _ => return "[unexpected]",
    };
    "[ok: " + d.root_cause + "]"
}

// The cross-provider guard is the design's Fallback-vs-continuity leak, made concrete:
// project the handle from an OpenAI-origin turn, try to resume it against a provider
// that expects Anthropic origin → a typed s07_CrossProviderContinuity, caught cleanly.
test "07: resuming foreign-origin state is a concrete, catchable CrossProviderContinuity" {
    let anthropic: baml.ai.Provider = s07_AnthropicReasoning {
        model: "claude-sonnet-4-5", api_key: "unused", max_tokens: 8192, thinking_budget: 4096,
        reply: "{\"root_cause\":\"x\",\"fix\":\"y\",\"confidence\":0.5}",
        scripted_thinking: "t", scripted_reasoning_tokens: 1,
    };
    // A handle whose origin is OpenAI, resumed on the Anthropic provider. The catch
    // arms unify with the try-value type (s07_Diagnosis); the outcome rides root_cause.
    let openai_state = s07_ContinuationState { openai_previous_response_id: "resp_1", resend_blocks: [], origin_provider: "openai" };
    let outcome = s07_resume_on<s07_Diagnosis>(anthropic, s07_Diagnose$render_prompt("go"), openai_state) catch (e) {
        let x: s07_CrossProviderContinuity => s07_Diagnosis { root_cause: "[cannot resume " + x.state_provider + " state on " + x.expected_provider + "]", fix: "", confidence: 0.0 },
        let c: s07_ContinuityError => s07_Diagnosis { root_cause: "[continuity: cross=" + c.is_cross_provider().to_string() + "]", fix: "", confidence: 0.0 },
        _ => s07_Diagnosis { root_cause: "[other]", fix: "", confidence: 0.0 },
    };
    assert.equal(outcome.root_cause, "[cannot resume openai state on anthropic]")
}

// ---------------------------------------------------------------------------
// 6 (design order): Combinator chain reads normally — reasoning providers are just
//    Providers. Because `$with` (call_messages_with) is an HttpProvider capability
//    that Fallback forwards, the reasoning projection routes through Fallback's
//    delegation and the sidecar is NOT dropped — the concrete win over the old
//    `.think` companion. NOTE the leak this still exposes (evaluation (d)): a handle
//    projected from one member can't be resumed on the other.
// ---------------------------------------------------------------------------
test "07: reasoning sidecar survives a Fallback (the win over the old .think companion)" {
    // First member always fails; fall back to a working reasoning provider. The
    // reasoning projection must still land (Fallback forwards call_messages_with).
    let resilient: baml.ai.Provider =
        AlwaysFailProvider {}.fallback_to(s07_OpenAiReasoning {
            model: "gpt-5.5", api_key: "unused",
            effort: s07_Effort.high, verbosity: s07_Verbosity.low, summary: "auto", include_encrypted: false,
            reply: "{\"root_cause\":\"fallback root cause\",\"fix\":\"f\",\"confidence\":0.7}",
            scripted_summary: "reasoned on the fallback member", scripted_reasoning_tokens: 90,
        });
    let r = s07_Diagnose$with(
        "flaky",
        (m: baml.ai.ResponseMeta) -> s07_ReasoningBlock[] | baml.ai.Unavailable {
            match (m) { let rm: s07_ReasoningMeta => rm.reasoning_blocks(), _ => baml.ai.Unavailable { reason: "none" } }
        },
        client = resilient,
    ) catch (e) {
        _ => baml.ai.CallResult<s07_Diagnosis, s07_ReasoningBlock[] | baml.ai.Unavailable> {
            value: s07_Diagnosis { root_cause: "fallback failed", fix: "", confidence: 0.0 }, meta: baml.ai.Unavailable { reason: "err" },
        },
    };
    assert.equal(r.value.root_cause, "fallback root cause")
    let blocks_ok = match (r.meta) { let b: s07_ReasoningBlock[] => b.length() > 0, let u: baml.ai.Unavailable => false };
    assert.is_true(blocks_ok)   // the sidecar was NOT dropped under Fallback
}

// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// Scenario 07 is a "tested" scenario (REALIZED.md): the reasoning-content-kind and
// Continuity surface are net-new and exercised offline above. The live tier proves
// only what the real stdlib surface supports today — a reasoning-capable model
// answering, with usage projected through the same `$with` sidecar. (Reasoning *text*
// is not yet surfaced by the stdlib OpenAiResponses meta, so we assert usage + answer.)
testset "integ-test-07-reasoning" {
    test "07 live: a reasoning model answers, usage projected via $with" {
        let r = s07_Diagnose$with(
            "The test intermittently fails only under parallelism. Reply with a JSON object with keys root_cause, fix, confidence.",
            (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
            client = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            },
        ) catch (e) {
            _ => baml.ai.CallResult<s07_Diagnosis, baml.ai.Usage> {
                value: s07_Diagnosis { root_cause: "live failed", fix: "", confidence: 0.0 },
                meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 },
            },
        };
        assert.is_true(r.value.root_cause != "live failed")
        assert.is_true(r.meta.output_tokens > 0)
    }
}
