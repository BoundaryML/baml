// scenario 10 — agentic loop + stop: ../../../../../llm-provider/ideas/scenarios/10-agentic-loop/README.md
//   usage.baml — APPLICATION-author code. End-user surface + runnable tests.
//
// The design's promise (background 02 §2 "the tool loop / agentic loop"): the
// multi-turn tool loop is FREE (the `Tools.run_tools` default method), stop policy
// (a numeric step cap, a `stopWhen` predicate, ORed predicates) lives in a
// `Bounded` combinator that overrides only the loop and never touches the wire,
// the budget hit returns a best-effort partial (`Budget<T>`, `step`'s `Item|Done`
// one level up), and swapping the client changes the provider with NOTHING else.
//
// On the real surface the design's `Bounded` IS `baml.ai.ToolLoop { inner, tools,
// dispatch, stop_when }`. Per the explicit "prefer LLM functions" directive the
// loop is spelled the design-true way — a plain LLM function called with a ToolLoop
// CLIENT (the loop is IMPLIED by the provider, not a call-site mode):
//     ResearchIt(q, client = baml.ai.ToolLoop { … })
// The budget-aware surface (`T | Budget<T> | HandoffRequest`) and the
// `Foo$run_tools` companion are the two explicit-control forms where the plain
// call's `-> T` return has no room for the extra arms.
//
// Live tier = testset "integ-test-10-agentic-loop" (native OpenAi tools + a cap).
//
// Fixtures live in implementation.baml (shared, read-only across scenarios):
// ScriptedTools / ScriptedNotTools / echo_dispatch / lookup_tools; NoCapProvider
// is common/fakes.baml. Preserves every D5 behavior: plain-call ToolLoop path,
// LoopBudgetExceeded typed throw, run_to_budget Budget arm, step_count_is /
// has_tool_call / any_of, $run_tools swapped-client + Unsupported + stop_when.


// ── (1) The trivial case: inherit the loop, drive it by handing a ToolLoop as
// the CLIENT of an ordinary LLM function. Zero loop code at the call site. ──────

// A dead declared client: never actually reached — every call below overrides it
// with `client = <ToolLoop>`. (`ResearchIt` + `DeadDeclared` are referenced from
// scenario 14; keep both names stable.)
client<llm> DeadDeclared {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ResearchIt(q: string) -> string {
  client DeadDeclared
  prompt #"Answer with tools: {{ q }}"#
}

test "10: a plain LLM-function call drives the ToolLoop to completion" {
    // The design's usage (1): the app writes NOTHING about the loop; the ToolLoop
    // client implies begin/step/dispatch/submit until a value falls out.
    let r = ResearchIt("go", client = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 2, final_json: "final answer" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    }) catch (e) {
        _ => "call failed",
    };
    assert.equal(r, "final answer")
}


// ── (2) Step cap. The design's `Bounded + step_count_is(n)`. On the plain call
// path the `-> T` return has no partial arm, so the budget hit is the typed
// `LoopBudgetExceeded` (use run_to_budget below to receive the value arm). ──────

test "10: step_count_is halts the plain path with a typed LoopBudgetExceeded" {
    let outcome = ResearchIt("go", client = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 5, final_json: "never reached" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: baml.ai.step_count_is(2),
    }) catch (e) {
        let b: baml.errors.LoopBudgetExceeded => "budget:" + b.steps_taken.to_string(),
        _ => "wrong error",
    };
    assert.equal(outcome, "budget:2")
}


// ── (3) Partial result at the budget. The design's "the limit signal must carry
// the last partial result": call the explicit `run_to_budget`, getting the honest
// SUM `T | Budget<T> | HandoffRequest` instead of a throw. ─────────────────────

test "10: run_to_budget returns the Budget sum arm instead of throwing" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 5, final_json: "never reached" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: baml.ai.step_count_is(1),
    };
    let outcome = agent.run_to_budget<string>("go") catch (e) {
        _ => "threw",
    };
    let label = match (outcome) {
        let b: baml.ai.Budget<string> => "stopped at " + b.steps_taken.to_string(),
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let s: string => s,
    };
    assert.equal(label, "stopped at 1")
}


// ── (4) stopWhen predicate, ORed. The design's
//     stopWhen: [ stepCountIs(99), hasToolCall("lookup") ]
// — `any_of` short-circuits on the model requesting the named tool. ────────────

test "10: has_tool_call inside any_of stops when the named tool is requested" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 3, final_json: "never reached" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: baml.ai.any_of([
            baml.ai.step_count_is(99),
            baml.ai.has_tool_call("lookup"),
        ]),
    };
    let outcome = agent.run_to_budget<string>("go") catch (e) {
        _ => "threw",
    };
    let label = match (outcome) {
        let b: baml.ai.Budget<string> => "stopped at " + b.steps_taken.to_string(),
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let s: string => s,
    };
    assert.equal(label, "stopped at 1")
}


// ── (5) Capability negotiation. A ToolLoop whose `inner` is not Tools-capable
// has no honest loop to drive -> a typed `Unsupported`, surfaced on the plain
// LLM-function call path. ──────────────────────────────────────────────────────

test "10: a non-Tools inner surfaces a typed Unsupported" {
    let outcome = ResearchIt("go", client = baml.ai.ToolLoop {
        inner: ScriptedNotTools {},
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    }) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(outcome, "unsupported")
}


// ── (6) Combinator chain, semantically checked. The design's `Bounded{…}
// .with_retry(2)`: `with_retry` is a Provider-marker factory inherited by the
// ToolLoop, but a re-driven loop replays every tool side effect, so `Retry`
// REFUSES to wrap an effectful provider — a typed `CannotRetry`, not a silent
// re-drive (the design's evaluation (c) "retry around a Tools loop is dubious",
// now enforced). Spelled the design-true way through the LLM function. ─────────

test "10: with_retry refuses to wrap the effectful loop (CannotRetry)" {
    let outcome = ResearchIt("go", client = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "fine" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    }.with_retry(2)) catch (e) {
        let c: baml.errors.CannotRetry => "refused",
        _ => "wrong error",
    };
    assert.equal(outcome, "refused")
}


// ── (7) The $run_tools companion (DCP §1.3): the explicit-control agentic form.
// Where the plain call implies the loop via a ToolLoop client, `Foo$run_tools`
// keeps the app's own provider client and swaps a Tools provider in per call —
// returning the honest sum `T | Budget<T> | HandoffRequest`. ───────────────────

test "10: Foo$run_tools drives the loop through a swapped-in Tools provider" {
    let agent = ScriptedTools { turns_wanted: 2, final_json: "companion answer" };
    let outcome = ResearchIt$run_tools("q", lookup_tools(), echo_dispatch, client = agent) catch (e) {
        _ => "companion failed",
    };
    let r = match (outcome) {
        let b: baml.ai.Budget<string> => "unexpected budget",
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let v: string => v,
    };
    assert.equal(r, "companion answer")
}

test "10: Foo$run_tools on a capability-less client is a typed Unsupported" {
    let outcome = ResearchIt$run_tools("q", lookup_tools(), echo_dispatch, client = NoCapProvider {}) catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    let r = match (outcome) {
        let v: string => v,
        _ => "wrong shape",
    };
    assert.equal(r, "unsupported")
}

test "10: Foo$run_tools with stop_when surfaces the Budget arm (D5)" {
    let agent = ScriptedTools { turns_wanted: 5, final_json: "never reached" };
    let outcome = ResearchIt$run_tools(
        "q",
        lookup_tools(),
        echo_dispatch,
        stop_when = baml.ai.step_count_is(1),
        client = agent,
    ) catch (e) {
        _ => "companion failed",
    };
    let r = match (outcome) {
        let b: baml.ai.Budget<string> => "budget@" + b.steps_taken.to_string(),
        let h: baml.ai.HandoffRequest => "unexpected handoff",
        let v: string => v,
    };
    assert.equal(r, "budget@1")
}
