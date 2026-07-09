// scenario 33 — evaluation: ../../../../../llm-provider/ideas/scenarios/33-evaluation/README.md
//
// The design: an eval loop is dataset + task + scorer(s) + runner, and the twist is
// that the TASK under test and the LLM-JUDGE scorer are BOTH just LLM functions. On
// the real surface:
//
//   - the TASK is a plain LLM function (s33_SupportReply) — the exact production body;
//   - the JUDGE is a plain LLM function that returns a TYPED Verdict via SAP
//     (s33_JudgeSupport -> s33_Verdict) — a different provider family, on purpose;
//   - DETERMINISTIC scorers are plain BAML functions (s33_mentions_policy / exact_match);
//   - the EVAL SUITE with a pass-threshold / tolerated-failures lands on a real
//     `testset ... with testing.PassRate(t)`: the runner middleware IS the CI gate,
//     failing cases under the threshold are TOLERATED, not hard failures.
//
// Live tier = testset "integ-test-33-evaluation": a real OpenAI task + a real
// Anthropic judge round-trip, the judge returning a typed Verdict on the task output.


// ---------------------------------------------------------------------------
// The eval vocabulary — plain data. The runner (a testset) speaks these; scorers
// produce them. No language change; library types.
// ---------------------------------------------------------------------------

// The judge's structured verdict — SAP parses the judge model's reply into this.
// Anchored 1..5 score plus a pass/fail (the background says pass/fail is steadier
// than a raw scale) and the judge's reasoning.
class s33_Verdict {
    pass: bool,
    score: int,        // anchored 1..5 against the rubric
    reasoning: string,
}

// A scorer's verdict — a continuous value AND a pass/fail, so a suite can gate on
// either. `rationale` is the judge's explanation; null for deterministic scorers.
class s33_Score {
    value: float,        // normalized 0.0..1.0
    passed: bool,
    scorer: string,      // which scorer produced this
    rationale: string?,
}

// Provenance — used to weight/slice cases. Golden = hand-curated must-pass;
// Holdout = withheld from tuning; Production = promoted from a recorded trace.
enum s33_CaseSource {
    Golden,
    Holdout,
    Production,
}

// One labelled example. `expected` is optional: reference-free scorers (the LLM
// judge, keyword asserts) have no golden string.
class s33_Case {
    id: string,
    input: string,
    expected: string?,
    source: s33_CaseSource,
}


// ---------------------------------------------------------------------------
// The TASK under test: a support-bot reply. A plain LLM function — the SAME body
// production serves — so the eval runs the exact code path. Offline tests swap in a
// deterministic EchoProvider; the live tier swaps in a real OpenAI provider.
//
// Default is a dead client so every offline test must pass an explicit provider.
// ---------------------------------------------------------------------------

client<llm> s33_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s33_SupportReply(question: string) -> string {
    client s33_Dead
    prompt #"
        {{ _.role("system") }}
        You are a concise support agent. Answer in one sentence.

        {{ _.role("user") }}
        {{ question }}
    "#
}


// ---------------------------------------------------------------------------
// The JUDGE: another plain LLM function, but its return type is the TYPED Verdict
// (structured output via SAP). DELIBERATELY a different family from the task
// (Anthropic vs OpenAI) to dodge self-preference bias. Temperature/model pinning
// (judge-drift defense) is a field on whatever provider is wired in.
// ---------------------------------------------------------------------------

function s33_JudgeSupport(question: string, answer: string, rubric: string) -> s33_Verdict {
    client s33_Dead
    prompt #"
        {{ _.role("system") }}
        You are a strict evaluator. {{ rubric }}
        Return a verdict: pass (true/false), score (an integer 1-5), and reasoning.

        {{ _.role("user") }}
        Question: {{ question }}
        Candidate answer: {{ answer }}
    "#
}

// The anchored rubric — calibration defense (anchors, not a bare 1..10 scale).
function s33_support_rubric() -> string {
    #"
        Score the support reply 1-5:
        5 = correct, complete, polite, no invented policy.
        3 = correct but missing a relevant detail or slightly off tone.
        1 = wrong, rude, or invents policy that does not exist.
        Pass iff score >= 4.
    "#
}


// ---------------------------------------------------------------------------
// DETERMINISTIC scorers — plain BAML functions, free, no model call. Reference-free
// (keyword assert) or reference-based (exact match).
// ---------------------------------------------------------------------------

// Reference-free code assertion: a refund answer must mention the 30-day window.
function s33_mentions_policy(output: string) -> s33_Score {
    let ok = output.includes("30 day") || output.includes("30-day");
    s33_Score {
        value: if (ok) { 1.0 } else { 0.0 },
        passed: ok,
        scorer: "mentions_policy",
        rationale: null,
    }
}

// Reference-based exact/substring match against the golden answer.
function s33_exact_match(output: string, expected: string) -> s33_Score {
    let ok = output.includes(expected) || output == expected;
    s33_Score {
        value: if (ok) { 1.0 } else { 0.0 },
        passed: ok,
        scorer: "exact_match",
        rationale: null,
    }
}


// ---------------------------------------------------------------------------
// The JUDGE scorer: the payoff of "the judge is just an LLM function". It calls the
// judge (with the given provider) and maps the typed Verdict onto a Score. A judge
// transport/parse fault is captured as a failing Score rather than aborting a sweep.
// ---------------------------------------------------------------------------

function s33_judge_score(question: string, answer: string, judge: baml.ai.Provider) -> s33_Score throws never {
    let v = s33_JudgeSupport(question, answer, s33_support_rubric(), client = judge) catch (e) {
        _ => return s33_Score { value: 0.0, passed: false, scorer: "llm_judge", rationale: "judge call failed" },
    };
    s33_Score {
        value: int_to_unit(v.score, 5),
        passed: v.pass,
        scorer: "llm_judge",
        rationale: v.reasoning,
    }
}

// v/max clamped to 0.0..1.0 — a small numeric helper (score 1..5 -> unit interval).
function int_to_unit(v: int, max: int) -> float {
    if (max <= 0) {
        0.0
    } else if (v <= 0) {
        0.0
    } else if (v >= max) {
        1.0
    } else {
        let vf = float.parse(string.from(v)) catch (e) { _ => 0.0 };
        let mf = float.parse(string.from(max)) catch (e) { _ => 1.0 };
        vf / mf
    }
}


// ---------------------------------------------------------------------------
// The dataset: golden cases. A real harness would also promote production traces
// (source = Production); here two hand-curated Golden/Holdout cases keep it offline.
// ---------------------------------------------------------------------------

function s33_golden_cases() -> s33_Case[] {
    [
        s33_Case {
            id: "refund-window",
            input: "How long do I have to return an item?",
            expected: "You have a 30 day return window from delivery.",
            source: s33_CaseSource.Golden,
        },
        s33_Case {
            id: "hours",
            input: "When is support open?",
            expected: "Support is available 24/7.",
            source: s33_CaseSource.Holdout,
        },
    ]
}


// ---------------------------------------------------------------------------
// OFFLINE tier — scorer + judge behavior, deterministic (EchoProvider).
// ---------------------------------------------------------------------------

test "33: a deterministic scorer gates task output on a policy keyword" {
    let out = s33_SupportReply(
        "How long do I have to return an item?",
        client = EchoProvider { reply: "You have a 30 day return window from delivery." },
    ) catch (e) {
        _ => "call failed",
    };
    log.info("33 task output: " + out);
    let s = s33_mentions_policy(out);
    log.info("33 score[" + s.scorer + "]: value=" + s.value.to_string() + " passed=" + s.passed.to_string());
    assert.is_true(s.passed)
}

test "33: an exact-match scorer is a plain reference-based function" {
    let cases = s33_golden_cases();
    let out = s33_SupportReply(
        cases[0].input,
        client = EchoProvider { reply: "You have a 30 day return window from delivery." },
    ) catch (e) { _ => "call failed" };
    let expected = match (cases[0].expected) {
        let x: string => x,
        _ => "",
    };
    let s = s33_exact_match(out, expected);
    log.info("33 exact_match: value=" + s.value.to_string() + " passed=" + s.passed.to_string());
    assert.is_true(s.passed)
}

test "33: the LLM judge returns a typed Verdict parsed via SAP" {
    // The judge is a plain LLM function; offline we feed its structured reply as
    // JSON through EchoProvider, and SAP parses it into the TYPED s33_Verdict.
    let v = s33_JudgeSupport(
        "How long do I have to return an item?",
        "You have a 30 day return window from delivery.",
        s33_support_rubric(),
        client = EchoProvider { reply: "{\"pass\": true, \"score\": 5, \"reasoning\": \"correct and complete\"}" },
    ) catch (e) {
        _ => s33_Verdict { pass: false, score: 0, reasoning: "judge failed" },
    };
    log.info("33 verdict: pass=" + v.pass.to_string() + " score=" + v.score.to_string() + " reason=" + v.reasoning);
    assert.is_true(v.pass);
    assert.equal(v.score, 5)
}

test "33: the judge scorer maps a Verdict onto a pass/fail Score" {
    let s = s33_judge_score(
        "How long do I have to return an item?",
        "You have a 30 day return window from delivery.",
        EchoProvider { reply: "{\"pass\": true, \"score\": 4, \"reasoning\": \"good\"}" },
    );
    log.info("33 judge score: value=" + s.value.to_string() + " passed=" + s.passed.to_string());
    assert.is_true(s.passed);
    // 4/5 = 0.8 on the unit interval.
    assert.is_true(s.value > 0.79 && s.value < 0.81)
}

test "33: a judge that declines a bad answer produces a failing Score" {
    let s = s33_judge_score(
        "How long do I have to return an item?",
        "I have no idea, figure it out yourself.",
        EchoProvider { reply: "{\"pass\": false, \"score\": 1, \"reasoning\": \"rude and unhelpful\"}" },
    );
    log.info("33 judge score (bad): value=" + s.value.to_string() + " passed=" + s.passed.to_string());
    assert.is_true(!s.passed)
}


// ---------------------------------------------------------------------------
// The EVAL SUITE as a CI gate. The design's "runner + gate on a pass-rate floor,
// tolerated failures" lands on a real `testset ... with testing.PassRate(t)`: the
// runner middleware aggregates the child pass-rate, and a failing case UNDER the
// threshold is TOLERATED (not a hard build failure). Here 3/4 pass = 75% >= 0.75,
// so the suite is GREEN with one tolerated regression — exactly the CI story.
// ---------------------------------------------------------------------------

testset "33 eval suite gates on a pass-rate threshold" with testing.PassRate(0.75) {
    test "33 suite: refund case clears the policy scorer" {
        let out = s33_SupportReply(
            "How long do I have to return an item?",
            client = EchoProvider { reply: "You have a 30 day return window from delivery." },
        ) catch (e) { _ => "call failed" };
        let s = s33_mentions_policy(out);
        log.info("33 suite refund: passed=" + s.passed.to_string());
        assert.is_true(s.passed)
    }

    test "33 suite: hours case clears the exact-match scorer" {
        let out = s33_SupportReply(
            "When is support open?",
            client = EchoProvider { reply: "Support is available 24/7." },
        ) catch (e) { _ => "call failed" };
        let s = s33_exact_match(out, "24/7");
        log.info("33 suite hours: passed=" + s.passed.to_string());
        assert.is_true(s.passed)
    }

    test "33 suite: judge approves a good answer" {
        let s = s33_judge_score(
            "How long do I have to return an item?",
            "You have a 30 day return window from delivery.",
            EchoProvider { reply: "{\"pass\": true, \"score\": 5, \"reasoning\": \"correct\"}" },
        );
        log.info("33 suite judge: passed=" + s.passed.to_string());
        assert.is_true(s.passed)
    }

    // A genuine regression: the answer omits the policy, so mentions_policy fails.
    // Under PassRate(0.75) this is TOLERATED — the suite stays green, the failure
    // is recorded but does not break the build.
    test "33 suite: a policy regression is tolerated under the threshold" {
        let out = s33_SupportReply(
            "How long do I have to return an item?",
            client = EchoProvider { reply: "You can return it whenever." },
        ) catch (e) { _ => "call failed" };
        let s = s33_mentions_policy(out);
        log.info("33 suite regression: passed=" + s.passed.to_string());
        assert.is_true(s.passed)
    }
}


// ---------------------------------------------------------------------------
// LIVE tier — a real OpenAI task + a real Anthropic judge round-trip. The judge
// returns a TYPED Verdict on the task's real output; different families dodge
// self-preference bias. Two sequential calls, well under 60s.
// ---------------------------------------------------------------------------

testset "integ-test-33-evaluation" {
    test "33 live: a real judge returns a typed Verdict on a real task's output" {
        let question = "How long do I have to return an item? Our policy is a 30 day return window.";
        let answer = s33_SupportReply(question, client = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        }) catch (e) {
            _ => "live task failed",
        };
        log.info("33 live task output: " + answer);

        let v = s33_JudgeSupport(question, answer, s33_support_rubric(), client = baml.ai.Anthropic {
            model: "claude-haiku-4-5-20251001",
            api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
            base_url: null,
            max_tokens: null,
            anthropic_version: null,
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }) catch (e) {
            _ => s33_Verdict { pass: false, score: 0, reasoning: "live judge failed" },
        };
        log.info("33 live verdict: pass=" + v.pass.to_string() + " score=" + v.score.to_string() + " reason=" + v.reasoning);

        // The typed Verdict round-tripped: a real judge, SAP-parsed into s33_Verdict,
        // with an anchored 1..5 score.
        assert.is_true(v.score >= 1 && v.score <= 5)
    }
}
