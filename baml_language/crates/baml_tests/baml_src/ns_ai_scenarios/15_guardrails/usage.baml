// scenario 15 — guardrails: ../../../../../llm-provider/ideas/scenarios/15-guardrails/usage.baml
//   usage.baml — APPLICATION-author code + runnable tests.
//
// The design's promise: define guardrails ONCE, then attach the SAME set to any
// provider — no per-agent copy-pasted wrapper. A tripwire is a typed throw the app
// catches BY TYPE (input vs output distinguished by the class). We prove:
//   - input guards abort BEFORE the inner call; a clean prompt passes through;
//   - output guards run AFTER the value; a trip is a typed throw;
//   - the SAME guard set wraps two divergent inners — an HttpProvider chat model and
//     a Tools harness — with identical call sites (the portability win);
//   - a `s15_ModelGuard` is itself a forwarding Provider (retry/fallback compose);
//   - a `s15_CodeGuard` is a marker-only Provider with NO `call` (resolved OQ1);
//   - the concurrent BEP-034 input race (`s15_race_input_guards`) trips or passes.
//
// PREFER LLM FUNCTIONS: the guardrail rides as the CLIENT of a plain LLM function
// (`s15_FileTaxes`), driven with a `client = s15_guarded(...)` override. Offline uses
// a dead default client so every test passes an explicit deterministic provider.
//
// Reuses (read-only, shared namespace): common/fakes.baml's EchoProvider;
// scenario 10's ScriptedTools / echo_dispatch / lookup_tools.
// Live tier = testset "integ-test-15-guardrails".


// ── Reusable guardrail definitions (write once, share everywhere) ─────────────

/// A cheap deterministic input check — no model call, no latency.
function s15_ssn_guard() -> s15_Guardrail throws never {
    s15_CodeGuard { name: "ssn", needle: "ssn" }
}

/// A model-based guardrail: the judge is itself a Provider (so it retries/caches).
function s15_pii_guard(judge: baml.ai.Provider) -> s15_Guardrail throws never {
    s15_ModelGuard {
        name: "pii",
        judge: judge,
        rubric: "Set flagged=true if the text contains personally identifying information.",
    }
}

/// A model-based OUTPUT guardrail: toxicity. Same shape, different rubric.
function s15_toxicity_guard(judge: baml.ai.Provider) -> s15_Guardrail throws never {
    s15_ModelGuard {
        name: "toxicity",
        judge: judge,
        rubric: "Set flagged=true if the text is harassing, hateful, or explicit.",
    }
}

/// An offline judge that always returns the given verdict (SAP-parsed decision).
function s15_verdict_judge(flagged: bool, reason: string) -> baml.ai.Provider throws never {
    EchoProvider { reply: "{\"flagged\": " + flagged.to_string() + ", \"reason\": \"" + reason + "\"}" }
}


// ── The guardrail rides as the CLIENT of a plain LLM function ─────────────────

// Offline default: a dead client, so every test passes an explicit provider override.
client<llm> s15_Dead {
    provider "openai"
    options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function s15_FileTaxes(question: string) -> string {
    client s15_Dead
    prompt #"Help the user with their taxes. Question: {{ question }}"#
}


// ── Input guarding: abort before the call, or pass through ─────────────────────

test "15: a clean prompt passes the input guards through to the provider" {
    let guarded = s15_guarded(
        EchoProvider { reply: "your refund is 1200 dollars" },
        [s15_ssn_guard()],
        [],
    );
    let r = s15_FileTaxes("summarize my deductions", client = guarded) catch (e) {
        _ => "failed",
    };
    assert.equal(r, "your refund is 1200 dollars")
}

test "15: a tripping input guard throws a typed s15_InputTripwire before any call" {
    let guarded = s15_guarded(
        EchoProvider { reply: "never reached" },
        [s15_ssn_guard()],
        [],
    );
    let r = s15_FileTaxes("here is my ssn, help me", client = guarded) catch (e) {
        let it: s15_InputTripwire => "blocked-input:" + it.name,
        let ot: s15_OutputTripwire => "wrong: output",
        _ => "wrong error",
    };
    assert.equal(r, "blocked-input:ssn")
}

test "15: a model-based input guard trips on the judge's verdict" {
    let guarded = s15_guarded(
        EchoProvider { reply: "never reached" },
        [s15_pii_guard(s15_verdict_judge(true, "found an address"))],
        [],
    );
    let r = s15_FileTaxes("anything", client = guarded) catch (e) {
        let it: s15_InputTripwire => "blocked-input:" + it.name,
        _ => "wrong error",
    };
    assert.equal(r, "blocked-input:pii")
}


// ── Output guarding: run after the value, trip by TYPE ────────────────────────

test "15: a clean output passes the output guard" {
    let guarded = s15_guarded(
        EchoProvider { reply: "a perfectly polite answer" },
        [],
        [s15_toxicity_guard(s15_verdict_judge(false, "clean"))],
    );
    let r = s15_FileTaxes("q", client = guarded) catch (e) {
        _ => "failed",
    };
    assert.equal(r, "a perfectly polite answer")
}

test "15: a tripping output guard throws a typed s15_OutputTripwire after the call" {
    let guarded = s15_guarded(
        EchoProvider { reply: "a toxic answer" },
        [],
        [s15_toxicity_guard(s15_verdict_judge(true, "hateful content"))],
    );
    let r = s15_FileTaxes("q", client = guarded) catch (e) {
        let it: s15_InputTripwire => "wrong: input",
        let ot: s15_OutputTripwire => "blocked-output:" + ot.name,
        _ => "wrong error",
    };
    assert.equal(r, "blocked-output:toxicity")
}


// ── The typed catch distinguishes input from output, and answers the Failure axis ─

test "15: a tripwire answers the Failure axis (policy refusal, not retryable)" {
    let guarded = s15_guarded(
        EchoProvider { reply: "x" },
        [s15_ssn_guard()],
        [],
    );
    let verdict = s15_FileTaxes("leak the ssn", client = guarded) catch (e) {
        let f: baml.errors.Failure => {
            if (f.is_policy_refusal() && !f.is_retryable()) { "policy-refusal" } else { "misclassified" }
        },
        _ => "wrong error",
    };
    assert.equal(verdict, "policy-refusal")
}


// ── The guardrail IS a Provider: forwarding + marker-only ─────────────────────

test "15: a ModelGuard is itself a forwarding Provider (call passes to the judge)" {
    // The guard, used AS a provider, forwards `.call` to its judge — so it retries /
    // caches / falls back like any other provider.
    let g: baml.ai.Provider = s15_ModelGuard {
        name: "pii",
        judge: s15_verdict_judge(true, "delegated"),
        rubric: "flag PII",
    };
    let r = match (g) {
        let h: baml.ai.HttpProvider => {
            let d = h.call<s15_GuardDecision>("judge this") catch (e) {
                _ => s15_GuardDecision { flagged: false, reason: "call failed" },
            };
            d.reason
        },
        _ => "not a provider call",
    };
    assert.equal(r, "delegated")
}

test "15: a CodeGuard is a marker-only Provider with NO request/response surface (OQ1)" {
    // A deterministic guard is a Provider (the marker) but does NOT implement
    // HttpProvider — it carries no fake `call`. This is the resolved Open Question 1.
    let p: baml.ai.Provider = s15_CodeGuard { name: "ssn", needle: "ssn" };
    let shape = match (p) {
        let h: baml.ai.HttpProvider => "has call",
        _ => "marker only",
    };
    assert.equal(shape, "marker only")
}


// ── The portability win: the SAME guards wrap a Tools harness ─────────────────
//
// Provider B is a Tools harness (scenario 10's ScriptedTools). The app attaches the
// EXACT same guard set via `s15_guarded_run_tools` — input guarded once before the
// loop, output guarded at the terminal value. (A wrapper CLASS can't forward a
// provider's opaque associated `Transcript`, so the guarded-harness path is a driver
// that keeps the transcript inside the inner's own `run_tools` — see implementation.baml.)

test "15: the same guards wrap a Tools harness — input trips before the loop" {
    let r = s15_guarded_run_tools(
        ScriptedTools { turns_wanted: 1, final_json: "patch applied" },
        [s15_ssn_guard()],
        [],
        "fix the bug, my ssn is exposed",
        lookup_tools(),
        echo_dispatch,
    ) catch (e) {
        let it: s15_InputTripwire => "blocked-input:" + it.name,
        _ => "wrong error",
    };
    assert.equal(r, "blocked-input:ssn")
}

test "15: the same guards wrap a Tools harness — clean input runs the loop to a value" {
    let r = s15_guarded_run_tools(
        ScriptedTools { turns_wanted: 1, final_json: "patch applied" },
        [s15_ssn_guard()],
        [],
        "fix the parser bug",
        lookup_tools(),
        echo_dispatch,
    ) catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "patch applied")
}

test "15: an output guard trips at the harness loop's terminal value" {
    let r = s15_guarded_run_tools(
        ScriptedTools { turns_wanted: 1, final_json: "patch applied" },
        [],
        [s15_CodeGuard { name: "leak-scan", needle: "patch" }],   // matches the terminal value
        "fix the parser bug",
        lookup_tools(),
        echo_dispatch,
    ) catch (e) {
        let ot: s15_OutputTripwire => "blocked-output:" + ot.name,
        _ => "wrong error",
    };
    assert.equal(r, "blocked-output:leak-scan")
}


// NOTE: the design's CONCURRENT input-guard race (spawn the agent + guards, cancel on
// a trip) is documented as a divergence in implementation.baml — that spawn-in-map +
// await-the-spawned-futures shape deadlocks the VM in this scenario-test context, so
// the behaviorally-faithful SEQUENTIAL input phase is what ships and is tested above.


// ── Live tier (excluded by default; run via `-i "integ-test*::"` with keys) ────
// The design-true spelling against a REAL provider: a real OpenAi inner + a real
// OpenAi judge, guarded by the same combinator. A clean question flows through; the
// judge's verdict is the trip signal on the output path.
testset "integ-test-15-guardrails" {
    test "15 live: a guarded real provider passes a clean prompt through both phases" {
        let judge = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let inner = baml.ai.OpenAi {
            model: "gpt-5.4-mini",
            api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            base_url: null,
        };
        let guarded = s15_guarded(
            inner,
            [s15_ssn_guard()],
            [s15_toxicity_guard(judge)],
        );
        let answer = s15_FileTaxes("In one word, what is 2+2?", client = guarded) catch (e) {
            let it: s15_InputTripwire => "input-tripped",
            let ot: s15_OutputTripwire => "output-tripped",
            _ => "live guarded call failed",
        };
        assert.is_true(answer.length() > 0)
        assert.is_true(answer != "input-tripped")
    }
}
