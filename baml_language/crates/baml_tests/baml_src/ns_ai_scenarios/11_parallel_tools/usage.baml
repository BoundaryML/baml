// scenario 11 — parallel tools: ../../../../../llm-provider/ideas/scenarios/11-parallel-tools/README.md
// Dispatch fans out over spawn + baml.future.all (the shipped BEP-034 stack).

function s11_parallel_dispatch(calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] throws never {
    let futures: baml.future.Future<baml.ai.ToolResult, null>[] = [];
    for (let c in calls) {
        futures
            .push(
                spawn {
            baml.ai.ToolResult { id: c.id, output: "result for " + c.name }
        },
            );
    }
    await baml.future.all(futures)
}

test "11: parallel dispatch fans out and preserves call identity" {
    let calls = [
        baml.ai.ToolCall { id: "a", name: "weather", args: "{}" },
        baml.ai.ToolCall { id: "b", name: "search", args: "{}" },
        baml.ai.ToolCall { id: "c", name: "math", args: "{}" },
    ];
    let results = s11_parallel_dispatch(calls);
    assert.equal(results.length(), 3)
    assert.equal(results[0].id, "a")
    assert.equal(results[1].output, "result for search")
    assert.equal(results[2].id, "c")
}

test "11: the parallel dispatcher plugs into a real ToolLoop run" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "fanned out" },
        tools: lookup_tools(),
        dispatch: s11_parallel_dispatch,
        stop_when: null,
    };
    let r = agent.call<string>("go") catch (e) {
        _ => "loop failed",
    };
    assert.equal(r, "fanned out")
}
