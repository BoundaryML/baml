test "guide 02.02: Agent runner owns the application tool loop" {
  let provider = fake_tool_provider([one_knowledge_call()]);
  let task = ResolveTicketWithTools_task(
    sample_ticket(),
    provider,
    [knowledge_tool()],
  );
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      tools = [knowledge_tool()],
      budget = root.ai.Budget { max_steps: 3, max_cost_usd: null },
    ),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => {
      assert.equal(done.value.summary, "Resolved with tools")
    },
    _ => assert.is_true(false),
  }
}

testset "integ-test-guide-02-agent-runner" {
  test "guide 02.02 live OpenAI: Agent runner manages the complete tool loop" {
    let hooks = no_op_hooks();
    let task = ResolveTicketWithTools_task(
      sample_ticket(),
      live_openai(),
      [knowledge_tool()],
    );
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(
        tools = [knowledge_tool()],
        budget = root.ai.Budget { max_steps: 4, max_cost_usd: null },
        hooks = hooks,
      ),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => {
        log.info("live Agent runner completed " + {
          "tool_calls": hooks.after_calls,
          "summary": done.value.summary,
        }.to_string());
        assert.equal(hooks.after_calls, ["search_knowledge"])
        assert.is_true(done.value.reply.length() > 0)
      },
      _ => assert.is_true(false),
    }
  }
}
