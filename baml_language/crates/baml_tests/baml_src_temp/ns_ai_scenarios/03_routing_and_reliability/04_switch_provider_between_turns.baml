test "guide 03.04: prepare_step can switch providers through transcript import" {
  let first = fake_tool_provider([one_knowledge_call()]);
  let second = root.ai.FakeToolProvider {
    ...fake_tool_provider([]),
    provider_name: "fake-tools-secondary",
  };
  let hooks = GuideHooks { ...no_op_hooks(), switch_after_step: second };
  let options = guide_agent_options([knowledge_tool()], guide_dispatch, hooks = hooks);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), first, [knowledge_tool()]),
    options,
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(
      done.transcript.provider().name(),
      "fake-tools-secondary",
    ),
    _ => assert.is_true(false),
  }
  assert.contains(hooks.hook_events.join(","), "provider_changed")
}
