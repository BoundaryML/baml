test "guide 06.02: an observer receives model, tool, and completion events" {
  //# Attach an observer to agent options
  let observer = GuideObserver { kinds: [] };
  let options = guide_agent_options(
    [knowledge_tool()],
    guide_dispatch,
    observers = [observer],
  );
  //# Run an observable tool loop
  root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  //# Verify model and completion events
  assert.contains(observer.kinds.join(","), "model_started")
  assert.contains(observer.kinds.join(","), "run_finished")
}
