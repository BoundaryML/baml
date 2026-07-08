// scenario 16 — agent security: ../../../../../llm-provider/ideas/scenarios/16-agent-security/README.md
// An allowlist gate slots into the dispatcher; a denied call is a distinct
// outcome, not a crash. (Compile-time info-flow is out of scope — plan §3.)

function security_gated_dispatch(
    calls: baml.ai.ToolCall[],
    allowed: string[],
) -> baml.ai.ToolResult[] throws never {
    let results: baml.ai.ToolResult[] = [];
    for (let c in calls) {
        let out = if (allowed.includes(c.name)) {
            "ran " + c.name
        } else {
            "DENIED (policy): " + c.name
        };
        results.push(baml.ai.ToolResult { id: c.id, output: out });
    }
    results
}

test "16: the allowlist gate denies out-of-policy calls as data" {
    let calls = [
        baml.ai.ToolCall { id: "1", name: "read_file", args: "{}" },
        baml.ai.ToolCall { id: "2", name: "exfiltrate", args: "{}" },
    ];
    let results = security_gated_dispatch(calls, ["read_file"]);
    assert.equal(results[0].output, "ran read_file")
    assert.is_true(results[1].output.includes("DENIED"))
}

test "16: the gate composes with a real ToolLoop run" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "safe outcome" },
        tools: lookup_tools(),
        dispatch: (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
            security_gated_dispatch(calls, ["lookup"])
        },
        stop_when: null,
    };
    let r = agent.call<string>("go") catch (e) { _ => "loop failed" };
    assert.equal(r, "safe outcome")
}
