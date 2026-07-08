// scenario 10 — agentic loop: ../../../../../llm-provider/ideas/scenarios/10-agentic-loop/README.md
// Offline tests for `baml.ai.ToolLoop` (DCP §1.2): the agentic loop as a
// provider — a plain HttpProvider call on a ToolLoop client drives
// begin/step/submit over the Tools-capable inner and returns the final value.

/// Scripted transcript: demand `turns_wanted` tool turns, then produce
/// `final_json` as the value (SAP-parsed into the caller's `T`).
class ScriptState {
    turns_wanted: int,
    turns_done: int,
    final_json: string,
}

/// An offline Tools-capable provider (no network): requests one `lookup` tool
/// call per turn until the script is exhausted, then answers.
class ScriptedTools {
    turns_wanted: int,
    final_json: string,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = ScriptState

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> ScriptState throws never {
            ScriptState {
                turns_wanted: self.turns_wanted,
                turns_done: 0,
                final_json: self.final_json,
            }
        }
        function step<T>(self, t: ScriptState) -> T | baml.ai.ToolCalls
            throws baml.errors.UnknownError {
            if (t.turns_done < t.turns_wanted) {
                return baml.ai.ToolCalls {
                    calls: [baml.ai.ToolCall { id: "call_1", name: "lookup", args: "{}" }],
                };
            }
            baml.sap.parse<T>(t.final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["script final parse failed"] },
            }
        }
        function submit(self, t: ScriptState, results: baml.ai.ToolResult[]) -> ScriptState throws never {
            ScriptState {
                turns_wanted: t.turns_wanted,
                turns_done: t.turns_done + 1,
                final_json: t.final_json,
            }
        }
    }
}

function echo_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
    let out: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        out.push(baml.ai.ToolResult { id: c.id, output: "looked-up" });
    }
    out
}

function lookup_tools() -> baml.ai.Tool[] {
    [baml.ai.Tool { name: "lookup", description: "Look something up.", parameters: "{}" }]
}

test "tool loop runs to completion through the plain call path" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 2, final_json: "final answer" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = agent.call<string>("go") catch (e) {
        _ => "call failed",
    };
    assert.equal(r, "final answer")
}

test "step_count_is halts the plain path with a typed LoopBudgetExceeded" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 5, final_json: "never reached" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: baml.ai.step_count_is(2),
    };
    let outcome = agent.call<string>("go") catch (e) {
        let b: baml.errors.LoopBudgetExceeded => "budget:" + b.steps_taken.to_string(),
        _ => "wrong error",
    };
    assert.equal(outcome, "budget:2")
}

test "run_to_budget returns the Budget sum arm instead of throwing" {
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
        let s: string => s,
    };
    assert.equal(label, "stopped at 1")
}

test "has_tool_call stops when the named tool is requested" {
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
        let s: string => s,
    };
    assert.equal(label, "stopped at 1")
}

test "non-Tools inner is a typed Unsupported" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedNotTools {},
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.call<string>("go") catch (e) {
        let u: baml.errors.Unsupported => "unsupported",
        _ => "wrong error",
    };
    assert.equal(outcome, "unsupported")
}

test "retry refuses to wrap the effectful loop" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "fine" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.with_retry(2).call<string>("go") catch (e) {
        let c: baml.errors.CannotRetry => "refused",
        _ => "wrong error",
    };
    assert.equal(outcome, "refused")
}

/// A provider with no Tools capability, for the negotiation-failure test.
class ScriptedNotTools {
    implements baml.ai.Provider {}
}

// ── The $run_tools companion (DCP §1.3): explicit-control agentic form ──────

client<llm> DeadDeclared {
  provider "openai"
  options { model "gpt-4o" api_key "unused" base_url "http://127.0.0.1:9" }
}

function ResearchIt(q: string) -> string {
  client DeadDeclared
  prompt #"Answer with tools: {{ q }}"#
}

test "Foo$run_tools drives the loop through a swapped-in Tools provider" {
    let agent = ScriptedTools { turns_wanted: 2, final_json: "companion answer" };
    // D5: the companion returns the honest sum `string | Budget<string>`.
    let outcome = ResearchIt$run_tools("q", lookup_tools(), echo_dispatch, client = agent) catch (e) {
        _ => "companion failed",
    };
    let r = match (outcome) {
        let b: baml.ai.Budget<string> => "unexpected budget",
        let v: string => v,
    };
    assert.equal(r, "companion answer")
}

test "Foo$run_tools on a capability-less client is a typed Unsupported" {
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

test "Foo$run_tools with stop_when surfaces the Budget arm (D5)" {
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
        let v: string => v,
    };
    assert.equal(r, "budget@1")
}
