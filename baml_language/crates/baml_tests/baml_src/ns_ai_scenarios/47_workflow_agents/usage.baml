// Scenario 47 remains a small compile reference so the 01-36 rewrite does not
// break the existing tail of the corpus. A whole root.ai Agent is a Provider,
// so a workflow activity can invoke it through the same Request<T> seam.

function s47_step_run_agent(agent: root.ai.Provider, task: string) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  let template = prompt`${task}`;
  root.ai.run<string>(root.ai.request<string>(agent, template))
}

test "47: a workflow step runs a nested agent (Agent as Provider)" {
  let agent = root.ai.Agent {
    inner: root.ai.FixtureTools {
      call_name: "get_weather",
      call_args: "{\"city\":\"Paris\"}",
      final_json: `"nested agent done"`,
    },
    tools: [s09_weather_tool()],
    dispatch: s09_weather_dispatch,
    stop_when: null,
  };
  let f = spawn { s47_step_run_agent(agent, "do the research step") };
  let r = (await f) catch (e) { _ => "step failed" };
  assert.equal(r, "nested agent done")
}
