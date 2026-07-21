test "guide 06.02: an observer receives model, tool, and completion events" {
  //# Attach an observer to agent options
  let observer = GuideObserver { kinds: [] };
  let options = root.ai.AgentOptions.new(
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

testset "integ-test-guide-06-observe-agent" {
  test "guide 06.02 live OpenAI: observer sees the exact model/tool/usage lifecycle" {
    let observer = GuideObserver { kinds: [] };
    let task = ToolInstructions_task(
      live_openai(),
      [knowledge_tool()],
      "Call search_knowledge exactly once for duplicate charges, then answer from its result.",
    );
    let outcome = root.ai.drivers.run_agent<Resolution>(
      task,
      root.ai.AgentOptions.new(
        [knowledge_tool()],
        guide_dispatch,
        observers = [observer],
      ),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
    log.info(baml.json.stringify({
      "guide": "06.02",
      "event_count": observer.kinds.length(),
      "events": observer.kinds.join(","),
    }));
    assert.equal(observer.kinds, [
      "model_started",
      "usage_updated",
      "tool_call_proposed",
      "tool_call_started",
      "tool_call_finished",
      "model_started",
      "usage_updated",
      "run_finished",
    ])
  }
}
