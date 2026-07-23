test "guide 02.02: Agent is a DriveProvider for direct-call lowering" {
  let tool_provider = fake_tool_provider([one_knowledge_call()]);
  let agent = root.ai.Agent {
    inner: tool_provider,
    tools: [knowledge_tool()],
    dispatch: guide_dispatch,
    budget: root.ai.Budget { max_steps: 3, max_cost_usd: null },
    hooks: null,
  };
  let value = ResolveTicketWithTools_manual(sample_ticket(), agent, [knowledge_tool()]);
  assert.equal(value.summary, "Resolved with tools")
}

testset "integ-test-guide-02-agent-drive-provider" {
  test "guide 02.02 live OpenAI: direct call delegates the complete tool loop to Agent" {
    let hooks = no_op_hooks();
    let agent = root.ai.Agent {
      inner: live_openai(),
      tools: [knowledge_tool()],
      dispatch: guide_dispatch,
      budget: root.ai.Budget { max_steps: 4, max_cost_usd: null },
      hooks: hooks,
    };
    let value = ResolveTicketWithTools_manual(sample_ticket(), agent, [knowledge_tool()]);
    log.info("live Agent drive completed " + {
      "tool_calls": hooks.after_calls,
      "summary": value.summary,
    }.to_string());
    assert.equal(hooks.after_calls, ["search_knowledge"])
    assert.is_true(value.reply.length() > 0)
  }
}
