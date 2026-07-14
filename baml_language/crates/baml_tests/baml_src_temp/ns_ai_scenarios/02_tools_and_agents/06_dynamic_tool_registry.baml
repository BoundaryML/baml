test "guide 02.06: a hook can add a tool before the next model step" {
  let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
  let hooks = no_op_hooks();
  hooks.registry = registry;
  hooks.tool_to_add = account_tool();
  let options = guide_agent_options([], guide_dispatch);
  options.tool_registry = registry;
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), []),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => match (done.transcript) {
      let transcript: root.ai.FakeToolTranscript => assert.equal(transcript.last_tools.length(), 2),
      _ => assert.is_true(false),
    },
    _ => assert.is_true(false),
  }
}
