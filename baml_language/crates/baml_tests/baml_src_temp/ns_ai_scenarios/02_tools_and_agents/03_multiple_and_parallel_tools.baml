test "guide 02.03: one model step may dispatch multiple tool calls" {
  let calls = [
    one_knowledge_call(),
    root.ai.ToolCall {
      id: "call-2", name: "lookup_account", args: { "customer_id": "C-1" },
    },
  ];
  let provider = fake_tool_provider(calls);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool(), account_tool()]),
    guide_agent_options([knowledge_tool(), account_tool()], guide_dispatch),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.priority, TicketPriority.Urgent),
    _ => assert.is_true(false),
  }
}
