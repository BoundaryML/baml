// scenario 32 — observability: ../../../../../llm-provider/ideas/scenarios/32-observability/README.md
//   usage.baml — APPLICATION-author code. End-user surface.
//
// The design's question: can the Provider/capability model express "see every
// model call as a trace, know its token cost, ship it to my backend"? On the
// REAL surface observability rides the VALUE path (not a host span exporter):
//
//   1. PER-CALL META PROJECTION — usage/finish_reason come back WITH the answer
//      out of ONE codec pass. The `Foo$with(args, project, client=)` companion
//      (desugar of `call_with`) returns `baml.ai.CallResult<T, V> { value, meta }`,
//      so there is no stateful `last_usage()` side-channel (the P1 fix).
//   2. USAGE NORMALIZATION — divergent providers (OpenAI prompt/completion vs
//      Anthropic input/output+cache) each normalize LAZILY into ONE `baml.ai.Usage`;
//      the app projects `m.usage()` and never touches the per-provider keys.
//   3. CHAIN-WIDE PROVENANCE — `baml.ai.UsageMeter` + `.traced(meter)` /
//      `meter.wrap(p)` records every SUCCESSFUL call's meta across a Fallback/
//      RoundRobin chain; `meter.total()` sums it, `meter.aggregate()` is itself a
//      `ResponseMeta` (`AggregateMeta`). The honest bar (plan D4 residue): a
//      FAILED attempt throws WITHOUT its meta, so a billed-but-failed member is
//      INVISIBLE — proven as a test below.
//   4. STRUCTURED LOGGING — `log.info(...)` is the design's event stream / exhaust.
//
// The design's host span exporter (`obs_start`/`obs_finish`/`obs_event`), the
// PII `redact()` seam, and the wire-vs-UI message split are NOT part of the
// realized D4 surface — they are net-new host boundaries; see the report's
// divergences. Observability here is the value-path residue that IS real.


// --- The single-turn function the whole scenario rides. ---------------------
// One plain LLM function; every observability policy is just a different Provider
// (or projection) handed in at the call site. Default is a dead client so offline
// tests must pass an explicit, deterministic override (ns_ai_scenarios house rule).
client<llm> DeadObs {
  provider openai
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function Summarize(q: string) -> string {
  client DeadObs
  prompt #"Summarize: {{ q }}"#
}


// ============================================================================
// 1. PER-CALL META PROJECTION — the `Foo$with` companion projects ResponseMeta
//    alongside the value (DCP §1.3): usage/finish_reason ride the value path.
//    [PRESERVED behaviors — the load-bearing $with / CallResult surface.]
// ============================================================================

test "32: Foo$with projects metadata alongside the value" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> string { m.finish_reason() },
        client = EchoProvider { reply: "summary!" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, string> { value: "with failed", meta: "" },
    };
    log.info("32 $with finish_reason: value=" + r.value + " meta=" + r.meta);
    assert.equal(r.value, "summary!")
    assert.equal(r.meta, "unknown")          // EchoProvider's LegacyMeta reports no finish reason
}

test "32: Foo$with usage projection over the echo fake is zeroed" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> int { m.usage().output_tokens },
        client = EchoProvider { reply: "s" },
    ) catch (e) {
        _ => baml.ai.CallResult<string, int> { value: "with failed", meta: -1 },
    };
    log.info("32 $with usage over LegacyMeta (zeroed): out=" + r.meta.to_string());
    assert.equal(r.meta, 0)
}

test "32: Foo$with on a capability-less client is a typed Unsupported" {
    let r = Summarize$with(
        "hello",
        (m: baml.ai.ResponseMeta) -> string { m.finish_reason() },
        client = NoCapProvider {},
    ) catch (e) {
        let u: baml.errors.Unsupported => baml.ai.CallResult<string, string> { value: "unsupported", meta: "" },
        _ => baml.ai.CallResult<string, string> { value: "wrong error", meta: "" },
    };
    log.info("32 $with negotiation-failure: " + r.value);
    assert.equal(r.value, "unsupported")
}


// ============================================================================
// 2. USAGE NORMALIZATION across DIVERGENT providers — the design's OpenAI-vs-
//    Anthropic story. Two fakes encode tokens under different internal keys
//    (prompt/completion vs input/output+cache); the app projects ONE `m.usage()`
//    and gets ONE normalized `baml.ai.Usage`, blind to the divergence.
// ============================================================================

test "32: $with over an OpenAI-style provider projects normalized usage" {
    let r = Summarize$with(
        "quarter",
        (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
        client = s32_MeteredEcho { reply: "revenue up", meta: s32_OpenAiStyleMeta { prompt: 120, completion: 40 } },
    ) catch (e) {
        _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } },
    };
    log.info("32 openai-style usage: in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string());
    assert.equal(r.value, "revenue up")
    assert.equal(r.meta.input_tokens, 120)
    assert.equal(r.meta.output_tokens, 40)
}

test "32: swapping to an Anthropic-style provider needs ZERO downstream change" {
    // Same projection (`m.usage()`); Anthropic-style folds its cache_read split
    // into input_tokens (design's cache split has no slot in the 2-field Usage).
    let r = Summarize$with(
        "quarter",
        (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
        client = s32_MeteredEcho { reply: "revenue up", meta: s32_AnthropicStyleMeta { input: 100, output: 40, cache_read: 20 } },
    ) catch (e) {
        _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } },
    };
    log.info("32 anthropic-style usage (cache folded): in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string());
    assert.equal(r.value, "revenue up")
    assert.equal(r.meta.input_tokens, 120)   // 100 fresh + 20 cache_read, normalized to one field
    assert.equal(r.meta.output_tokens, 40)
}

test "32: one call, both the app's projection AND the finish reason ride the value path" {
    // The projection can compose several dimensions in one codec pass.
    let r = Summarize$with(
        "quarter",
        (m: baml.ai.ResponseMeta) -> string { m.finish_reason() + "/" + m.usage().output_tokens.to_string() },
        client = s32_MeteredEcho { reply: "done", meta: s32_OpenAiStyleMeta { prompt: 5, completion: 9 } },
    ) catch (e) {
        _ => baml.ai.CallResult<string, string> { value: "failed", meta: "" },
    };
    log.info("32 composed projection: " + r.meta);
    assert.equal(r.meta, "stop/9")
}


// ============================================================================
// 3. CHAIN-WIDE PROVENANCE — a `UsageMeter` wraps each member of a chain and
//    records every SUCCESSFUL call's meta, so an app observes usage over
//    everything that ACTUALLY RAN, driven through the plain LLM function.
// ============================================================================

test "32: .traced(meter) records a plain call's usage as a transparent side effect" {
    let meter = baml.ai.UsageMeter { entries: [] };
    // `.traced(meter)` forwards to the inner provider unchanged and records its
    // meta; the value the app gets is untouched (tracing is a side effect).
    let prov: baml.ai.Provider =
        s32_MeteredEcho { reply: "traced answer", meta: s32_OpenAiStyleMeta { prompt: 30, completion: 12 } }.traced(meter);
    let r = Summarize("summarize", client = prov) catch (e) { _ => "failed" };
    let total = meter.total();
    log.info("32 traced single call: reply=" + r + " calls=" + meter.calls().to_string() + " in=" + total.input_tokens.to_string() + " out=" + total.output_tokens.to_string());
    assert.equal(r, "traced answer")
    assert.equal(meter.calls(), 1)
    assert.equal(total.input_tokens, 30)
    assert.equal(total.output_tokens, 12)
}

test "32: one meter sums usage across every member that ran under a RoundRobin" {
    let meter = baml.ai.UsageMeter { entries: [] };
    let pool: baml.ai.Provider = baml.ai.RoundRobin {
        members: [
            meter.wrap(s32_MeteredEcho { reply: "A", meta: s32_OpenAiStyleMeta { prompt: 1, completion: 2 } }),
            meter.wrap(s32_MeteredEcho { reply: "B", meta: s32_AnthropicStyleMeta { input: 10, output: 20, cache_read: 0 } }),
        ],
        counter: 0,
    };
    let r1 = Summarize("q1", client = pool) catch (e) { _ => "failed" };
    let r2 = Summarize("q2", client = pool) catch (e) { _ => "failed" };
    let total = meter.total();
    log.info("32 round-robin provenance: r1=" + r1 + " r2=" + r2 + " calls=" + meter.calls().to_string() + " in=" + total.input_tokens.to_string() + " out=" + total.output_tokens.to_string());
    assert.equal(r1, "A")
    assert.equal(r2, "B")
    assert.equal(meter.calls(), 2)
    assert.equal(total.input_tokens, 11)     // 1 + 10 across the two members that ran
    assert.equal(total.output_tokens, 22)    // 2 + 20
    // The meter's aggregate is ITSELF a ResponseMeta (AggregateMeta): sum usage,
    // last part's finish_reason.
    assert.equal(meter.aggregate().usage().output_tokens, 22)
    assert.equal(meter.aggregate().finish_reason(), "end_turn")   // the last member that ran
}


// ============================================================================
// 4. THE HONEST BAR (plan D4 residue) — winner-ONLY observability. Under a
//    Fallback, a FAILED attempt throws WITHOUT carrying its meta, so the tokens
//    you were billed for on the failing member are INVISIBLE to the meter. The
//    span/meter sees the WINNER's usage, not the chain's true cost. This is the
//    design's acknowledged Fallback-aggregate gap, made a test.
// ============================================================================

test "32: a meter over a Fallback observes only the WINNER (failed members carry no meta)" {
    let meter = baml.ai.UsageMeter { entries: [] };
    let chain: baml.ai.Provider = baml.ai.Fallback {
        members: [
            // This member fails — and its (billed) usage is LOST to observability.
            meter.wrap(AlwaysFailProvider {}),
            // The winner: its meta IS recorded.
            meter.wrap(s32_MeteredEcho { reply: "recovered", meta: s32_OpenAiStyleMeta { prompt: 8, completion: 4 } }),
        ],
    };
    let r = Summarize("summarize", client = chain) catch (e) { _ => "chain failed" };
    let total = meter.total();
    log.info("32 winner-only provenance: reply=" + r + " observed_calls=" + meter.calls().to_string() + " in=" + total.input_tokens.to_string() + " out=" + total.output_tokens.to_string());
    assert.equal(r, "recovered")
    // ONLY the winning call was observed — the failed member is invisible.
    assert.equal(meter.calls(), 1)
    assert.equal(total.input_tokens, 8)
    assert.equal(total.output_tokens, 4)
}


// ============================================================================
// 5. AGGREGATE PROVENANCE as a standalone ResponseMeta — `AggregateMeta` sums
//    usage across parts and reports the LAST part's finish_reason (the one whose
//    value the caller got). Useful to fold a hand-collected set of metas.
// ============================================================================

test "32: AggregateMeta sums usage across parts; finish_reason is the last part's" {
    let agg = baml.ai.AggregateMeta { parts: [
        s32_OpenAiStyleMeta { prompt: 2, completion: 3 },
        s32_AnthropicStyleMeta { input: 5, output: 7, cache_read: 0 },
    ] };
    log.info("32 aggregate: in=" + agg.usage().input_tokens.to_string() + " out=" + agg.usage().output_tokens.to_string() + " finish=" + agg.finish_reason());
    assert.equal(agg.usage().input_tokens, 7)
    assert.equal(agg.usage().output_tokens, 10)
    assert.equal(agg.finish_reason(), "end_turn")   // the last part's
    let empty = baml.ai.AggregateMeta { parts: [] };
    assert.equal(empty.usage().input_tokens, 0)
    assert.equal(empty.finish_reason(), "unknown")
}


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ──
// The design's claim against the REAL API: usage/finish_reason ride the value
// path, and a `.traced(meter)` chain aggregates real usage.
testset "integ-test-32-observability" {
    test "32 live: $with over real OpenAI projects real usage (input+output tokens > 0)" {
        let real: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let r = Summarize$with(
            "Reply with exactly the lowercase word: pong",
            (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
            client = real,
        ) catch (e) {
            _ => baml.ai.CallResult<string, baml.ai.Usage> { value: "live $with failed", meta: baml.ai.Usage { input_tokens: -1, output_tokens: -1 } },
        };
        log.info("32 live $with usage: value=" + r.value + " in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string());
        assert.is_true(r.meta.input_tokens > 0)
        assert.is_true(r.meta.output_tokens > 0)
    }

    test "32 live: a .traced(meter) real call records real usage into the meter" {
        let meter = baml.ai.UsageMeter { entries: [] };
        let real: baml.ai.Provider = baml.ai.OpenAi {
            model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null,
            extra_headers: null, extra_body: null, append_output_schema: null,
        };
        let r = Summarize("Reply with exactly the lowercase word: pong", client = real.traced(meter)) catch (e) {
            _ => "live traced failed",
        };
        let total = meter.total();
        log.info("32 live traced meter: reply=" + r + " calls=" + meter.calls().to_string() + " in=" + total.input_tokens.to_string() + " out=" + total.output_tokens.to_string());
        assert.equal(meter.calls(), 1)
        assert.is_true(total.input_tokens > 0)
        assert.is_true(total.output_tokens > 0)
    }
}
