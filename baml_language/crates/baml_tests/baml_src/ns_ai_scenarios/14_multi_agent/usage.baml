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
