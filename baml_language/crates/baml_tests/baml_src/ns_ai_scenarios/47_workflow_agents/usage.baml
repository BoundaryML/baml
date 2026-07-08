// scenario 47 — agents in workflows: ../../../../../llm-provider/ideas/scenarios/47-workflow-agent-nesting/README.md
// A whole agent (ToolLoop) is a Provider, so a workflow step can run one —
// nesting is plain composition.

function s47_step_run_agent(agent: baml.ai.Provider, task: string) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
    match (agent) {
        let h: baml.ai.HttpProvider => h.call<string>(task),
        _ => throw baml.errors.Unsupported { message: "step's agent is not runnable" },
    }
}

test "47: a workflow step runs a nested agent (ToolLoop as Provider)" {
    let agent = baml.ai.ToolLoop {
        inner: ScriptedTools { turns_wanted: 1, final_json: "nested agent done" },
        tools: lookup_tools(),
        dispatch: echo_dispatch,
        stop_when: null,
    };
    let f = spawn { s47_step_run_agent(agent, "do the research step") };
    let r = (await f) catch (e) { _ => "step failed" };
    assert.equal(r, "nested agent done")
}
