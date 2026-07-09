// scenario 30 — cascades & routing: ../../../../../llm-provider/ideas/scenarios/30-cascades-routing/README.md
//   usage.baml — APPLICATION-author tests. Offline tier "30: …"; live tier below.
//
// Escalate to the expensive tier only when the cheap one is unsure or its answer
// fails a checkable gate. Escalation is QUALITY-driven (a successful cheap answer
// can still escalate) — the load-bearing distinction from 29's failure-driven
// Fallback. The presence-not-calibration caveat (plan D3.iv) is inherent: a
// self-reported confidence can silently never escalate; we surface `tier`/`source`
// so the regression is observable, not silent (design evaluation §c).


// ── (1) Checkable-gate cascade ──────────────────────────────────────────────────

test "30: a cheap answer that passes the gate never escalates" {
    let out = s30_gated_cascade(
        EchoProvider { reply: "the capital is Paris" },
        s30_CountingProvider { reply: "STRONG ANSWER", calls: 0 },
        "capital of France?",
        "paris",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "the capital is Paris")
    assert.equal(out.escalated, false)
    assert.equal(out.tier, "cheap")
}

test "30: a cheap answer that FAILS the gate escalates — even though nothing threw" {
    // The quality-driven distinction from Fallback: the cheap call SUCCEEDS, but its
    // answer misses the required token, so the cascade still reaches the strong tier.
    let out = s30_gated_cascade(
        EchoProvider { reply: "I am not sure" },
        EchoProvider { reply: "the capital is Paris" },
        "capital of France?",
        "paris",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "the capital is Paris")
    assert.is_true(out.escalated)
    assert.equal(out.tier, "strong")
}

test "30: the cheap path bills ONLY the cheap tier (cost story)" {
    // The strong tier is a counting provider: a settled cascade must leave its
    // call count at zero — "only the residual pays the premium".
    let strong = s30_CountingProvider { reply: "STRONG", calls: 0 };
    let out = s30_gated_cascade(
        EchoProvider { reply: "answer: 42" },
        strong,
        "meaning of life?",
        "42",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.escalated, false)
    assert.equal(strong.calls, 0)
}

test "30: an escalation DOES bill the strong tier once" {
    let strong = s30_CountingProvider { reply: "STRONG-42", calls: 0 };
    let out = s30_gated_cascade(
        EchoProvider { reply: "no idea" },
        strong,
        "meaning of life?",
        "42",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.is_true(out.escalated)
    assert.equal(strong.calls, 1)
}


// ── (2) Confidence-gated cascade (design's Scored signal as a plain field) ───────

test "30: a confident cheap answer settles on the cheap tier" {
    let out = s30_confidence_cascade(
        EchoProvider { reply: "{\"answer\": \"Paris\", \"confidence\": 0.95}" },
        EchoProvider { reply: "STRONG: Paris" },
        "capital of France?",
        0.7,
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "Paris")
    assert.equal(out.escalated, false)
}

test "30: a low-confidence cheap answer escalates to the strong tier" {
    let out = s30_confidence_cascade(
        EchoProvider { reply: "{\"answer\": \"maybe Lyon?\", \"confidence\": 0.3}" },
        EchoProvider { reply: "STRONG: Paris" },
        "capital of France?",
        0.7,
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "STRONG: Paris")
    assert.is_true(out.escalated)
}


// ── (3) Judge-gated escalation ──────────────────────────────────────────────────

test "30: the judge accepts a good cheap candidate (no escalation)" {
    let out = s30_judge_gated(
        EchoProvider { reply: "Paris" },
        EchoProvider { reply: "{\"score\": 0.9, \"reason\": \"correct\"}" },
        EchoProvider { reply: "STRONG: Paris" },
        "capital of France?",
        0.6,
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "Paris")
    assert.equal(out.escalated, false)
}

test "30: the judge rejects a weak cheap candidate and escalates" {
    let out = s30_judge_gated(
        EchoProvider { reply: "Lyon" },
        EchoProvider { reply: "{\"score\": 0.2, \"reason\": \"wrong city\"}" },
        EchoProvider { reply: "STRONG: Paris" },
        "capital of France?",
        0.6,
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "STRONG: Paris")
    assert.is_true(out.escalated)
}


// ── (4) Semantic / classifier routing ───────────────────────────────────────────

test "30: the task-class map routes each class to its tier (plain BAML, no LLM)" {
    let easy: baml.ai.Provider = EchoProvider { reply: "cheap" };
    let hard: baml.ai.Provider = EchoProvider { reply: "expensive" };
    let pe = s30_route(s30_TaskClass.easy, easy, hard);
    let ph = s30_route(s30_TaskClass.hard, easy, hard);
    let re = match (pe) { let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "err" }, _ => "not http" };
    let rh = match (ph) { let h: baml.ai.HttpProvider => h.call<string>("q") catch (e) { _ => "err" }, _ => "not http" };
    assert.equal(re, "cheap")
    assert.equal(rh, "expensive")
}

test "30: a classifier picks the tier BEFORE generation (routing = client-as-function)" {
    // classifier says "hard" -> the hard tier answers; the easy tier is never called.
    let easy = s30_CountingProvider { reply: "EASY", calls: 0 };
    let out = s30_routed_answer(
        EchoProvider { reply: "hard" },
        easy,
        EchoProvider { reply: "HARD ANSWER" },
        "prove the Riemann hypothesis",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "HARD ANSWER")
    assert.equal(out.tier, "hard")
    assert.equal(easy.calls, 0)
}

test "30: an easy request routes to the cheap tier" {
    let out = s30_routed_answer(
        EchoProvider { reply: "easy" },
        EchoProvider { reply: "EASY ANSWER" },
        EchoProvider { reply: "HARD ANSWER" },
        "what's 2+2",
    ) catch (e) { _ => s30_CascadeOutcome { answer: "failed", escalated: false, tier: "err" } };
    assert.equal(out.answer, "EASY ANSWER")
    assert.equal(out.tier, "easy")
}


// ── Live tier — a REAL 2-stage cascade (excluded by default; run with keys) ─────
// gpt-5.4-mini answers first; we escalate to the bigger gpt-5.4 ONLY when the cheap
// answer fails a checkable gate. Both calls are tiny (one short line) — under 60s.
testset "integ-test-30-cascades-routing" {
    test "30 live: the cheap tier settles when its answer passes the gate" {
        // gpt-5.4-mini reliably echoes "pong"; the gate passes, no escalation.
        let out = s30_gated_cascade(
            baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null },
            baml.ai.OpenAi { model: "gpt-5.4", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null },
            "Reply with exactly the lowercase word: pong",
            "pong",
        ) catch (e) { _ => s30_CascadeOutcome { answer: "live failed", escalated: false, tier: "err" } };
        log.info("30 live cheap-settles: tier=" + out.tier + " answer=" + out.answer);
        assert.is_true(out.answer.to_lower_case().includes("pong"))
        assert.equal(out.escalated, false)
        assert.equal(out.tier, "cheap")
    }

    test "30 live: a failed gate escalates to the strong tier" {
        // The gate demands a token gpt-5.4-mini won't produce for this prompt, so the
        // cascade escalates; gpt-5.4 then answers "pong". Proves the escalation path
        // end-to-end with two real, tiny calls.
        let out = s30_gated_cascade(
            baml.ai.OpenAi { model: "gpt-5.4-mini", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null },
            baml.ai.OpenAi { model: "gpt-5.4", api_key: baml.env.get_or_panic("OPENAI_API_KEY"), base_url: null },
            "Reply with exactly the lowercase word: pong",
            "escalate-sentinel-xyz",
        ) catch (e) { _ => s30_CascadeOutcome { answer: "live failed", escalated: false, tier: "err" } };
        log.info("30 live escalates: tier=" + out.tier + " answer=" + out.answer);
        assert.is_true(out.escalated)
        assert.equal(out.tier, "strong")
        assert.is_true(out.answer.length() > 0 && out.answer != "live failed")
    }
}
