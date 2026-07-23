testset "integ-test-guide-02-agent-capabilities" {
  test "guide 02 live OpenAI: dynamic tools, hooks, events, and parallel-capable dispatch" {
    //# Configure the native provider
    let provider = root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    //# Build a dynamic registry, hooks, and observer
    let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
    let hooks = GuideHooks { ...no_op_hooks(), tool_to_add: account_tool() };
    let observer = GuideObserver { kinds: [] };
    let options = root.ai.AgentOptions.new(
      [],
      guide_dispatch,
      tool_registry = registry,
      hooks = hooks,
      observers = [observer],
    );
    //# Run the live agent with dynamic capabilities
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, []),
      options,
    );
    //# Inspect transcript and emitted events
    match (outcome) {
      let done: root.ai.Done<Resolution> => match (done.transcript) {
        let transcript: root.ai.OpenAiToolTranscript => assert.equal(transcript.tools_json.length(), 2),
        _ => assert.is_true(false),
      },
      _ => assert.is_true(false),
    }
    assert.contains(observer.kinds.join(","), "run_finished")
    assert.contains(hooks.hook_events.join(","), "model_started")
  }

  test "guide 02 live OpenAI: handoff stops before application dispatch" {
    //# Configure a native provider and handoff tool
    let provider = root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    let handoff = account_tool().as_handoff();
    //# Run until the handoff boundary
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [handoff]),
      root.ai.AgentOptions.new([handoff], guide_dispatch),
    );
    //# Verify control was transferred
    match (outcome) {
      let value: root.ai.Handoff => assert.equal(value.to, "lookup_account"),
      _ => assert.is_true(false),
    }
  }
}
