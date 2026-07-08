// scenario 14 — multi-agent handoff: ../../../../../llm-provider/ideas/scenarios/14-multi-agent/README.md
// A sub-agent is itself a Provider: a "handoff" is calling another provider
// inside the dispatcher — deterministic orchestration is plain composition.

function s14_handoff_dispatch(
    specialist: baml.ai.Provider,
    calls: baml.ai.ToolCall[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = match (specialist) {
            let h: baml.ai.HttpProvider => {
                h.call<string>(c.args) catch (e) {
                    _ => "sub-agent failed"
                }
            },
            _ => "specialist is not callable",
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}

test "14: the dispatcher hands off to a specialist provider" {
    let specialist: baml.ai.Provider = EchoProvider { reply: "specialist verdict" };
    let results = s14_handoff_dispatch(specialist, [baml.ai.ToolCall { id: "h1", name: "escalate", args: "case details" }]);
    assert.equal(results.length(), 1)
    assert.equal(results[0].output, "specialist verdict")
}

test "14: a capability-less specialist degrades to a typed message, not a crash" {
    let results = s14_handoff_dispatch(NoCapProvider {}, [baml.ai.ToolCall { id: "h1", name: "escalate", args: "x" }]);
    assert.equal(results[0].output, "specialist is not callable")
}

// ── D5 slice 2: the HONEST handoff — a first-class HandoffRequest return arm ──
// A tool marked `handoff: true` is a transfer, not a dispatch: the loop ENDS
// and the CALLER routes. (The dispatcher-delegation pattern above remains the
// right shape when the original agent should keep owning the conversation.)

function s14_transfer_tools() -> baml.ai.Tool[] {
    [baml.ai.Tool {
        name: "lookup",
        description: "Transfer this conversation to the lookup specialist.",
        parameters: "{}",
        handoff: true,
    }]
}

test "14: a handoff-marked tool call returns the HandoffRequest arm (run_to_budget)" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 3, final_json: "never reached" },
        tools: s14_transfer_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let outcome = agent.run_to_budget<string>("route me") catch (e) {
        _ => "loop failed",
    };
    let r = match (outcome) {
        let h: baml.ai.HandoffRequest => "handoff->" + h.to + "@" + h.steps_taken.to_string(),
        let b: baml.ai.Budget<string> => "unexpected budget",
        let v: string => v,
    };
    assert.equal(r, "handoff->lookup@1")
}

test "14: the plain LLM-function path surfaces an unresolved handoff as a TYPED throw" {
    // Design-true spelling: a plain LLM-function call with the loop implied by
    // the ToolLoop CLIENT (DCP §1.6) — the desugared path must produce T, so
    // an unresolved handoff is the typed HandoffUnresolved error here.
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 3, final_json: "never reached" },
        tools: s14_transfer_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let r = ResearchIt("route me", client = agent) catch (e) {
        let h: baml.errors.HandoffUnresolved => "unresolved->" + h.to,
        _ => "wrong error",
    };
    assert.equal(r, "unresolved->" + "lookup")
}

test "14: run_tools surfaces the HandoffRequest arm on the interface loop too" {
    let p: baml.ai.Provider = ScriptedTools { turns_wanted: 3, final_json: "never reached" };
    let r = match (p) {
        let tp: baml.ai.Tools => {
            let outcome = tp.run_tools<string>("route me", s14_transfer_tools(), echo_dispatch) catch (e) {
                _ => "loop failed"
            };
            match (outcome) {
                let h: baml.ai.HandoffRequest => "handoff->" + h.to,
                _ => "no handoff",
            }
        },
        _ => "no tools capability",
    };
    assert.equal(r, "handoff->lookup")
}
