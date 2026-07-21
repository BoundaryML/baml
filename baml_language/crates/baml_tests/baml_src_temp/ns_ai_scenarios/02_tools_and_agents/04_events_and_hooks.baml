test "guide 02.04: observers and post-tool hooks see the run" {
  let observer = GuideObserver { kinds: [] };
  let hooks = no_op_hooks();
  let options = root.ai.AgentOptions.new(
    [knowledge_tool()],
    guide_dispatch,
    hooks = hooks,
    observers = [observer],
  );
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), fake_tool_provider([one_knowledge_call()]), [knowledge_tool()]),
    options,
  );
  assert.contains(observer.kinds.join(","), "tool_call_started")
  assert.equal(hooks.after_calls, ["search_knowledge"])
  assert.contains(hooks.hook_events.join(","), "run_finished")
}

testset "integ-test-guide-02-events-and-hooks" {
  test "guide 02.04 live OpenAI: observer and hooks see the real tool lifecycle" {
    let observer = GuideObserver { kinds: [] };
    let hooks = no_op_hooks();
    let provider = live_openai();
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ToolInstructions_task(
        provider,
        [knowledge_tool()],
        `Call search_knowledge exactly once with query "duplicate charge", then use its result to return the final resolution.`,
      ),
      root.ai.AgentOptions.new(
        [knowledge_tool()],
        guide_dispatch,
        hooks = hooks,
        observers = [observer],
      ),
    );
    log.info("live agent events " + {
      "observer": observer.kinds,
      "hook_events": hooks.hook_events,
      "after_calls": hooks.after_calls,
    }.to_string());
    assert.equal(hooks.after_calls, ["search_knowledge"])
    assert.contains(observer.kinds.join(","), "tool_call_proposed")
    assert.contains(observer.kinds.join(","), "tool_call_started")
    assert.contains(observer.kinds.join(","), "tool_call_finished")
    assert.contains(observer.kinds.join(","), "usage_updated")
    assert.contains(observer.kinds.join(","), "run_finished")
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
