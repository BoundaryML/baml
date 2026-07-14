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
