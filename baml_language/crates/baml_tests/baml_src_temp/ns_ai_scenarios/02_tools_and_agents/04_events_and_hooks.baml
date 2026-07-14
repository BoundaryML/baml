test "guide 02.04: observers and post-tool hooks see the run" {
  let observer = GuideObserver { kinds: [] };
  let hooks = no_op_hooks();
  let options = guide_agent_options([knowledge_tool()], guide_dispatch);
  options.observers = [observer];
  options.hooks = hooks;
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  assert.contains(observer.kinds.join(","), "tool_call_started")
  assert.equal(hooks.after_calls, ["search_knowledge"])
  assert.contains(hooks.hook_events.join(","), "run_finished")
}
