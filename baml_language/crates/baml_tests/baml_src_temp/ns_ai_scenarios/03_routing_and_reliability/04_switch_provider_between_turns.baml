test "guide 03.04: prepare_step can switch providers through transcript import" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = fake_tool_provider([]);
  let hooks = no_op_hooks();
  hooks.switch_after_step = second;
  let options = guide_agent_options([knowledge_tool()], guide_dispatch);
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), first, [knowledge_tool()]),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.transcript.provider().name(), "fake-tools"),
    _ => assert.is_true(false),
  }
}
