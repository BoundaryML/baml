testset "integ-test-guide-02-agent-capabilities" {
  test "guide 02 live OpenAI: dynamic tools, hooks, events, and parallel-capable dispatch" {
    //# Configure the native provider
    let provider = root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    //# Build a dynamic registry, hooks, and observer
    let registry = root.ai.ToolRegistry.new([search_knowledge]);
    let hooks = GuideHooks {
      ...no_op_hooks(),
      tool_to_add: root.ai.normalize_tool(lookup_account),
    };
    let observer = GuideObserver { kinds: [] };
    let runner = root.ai.run.Agent<Resolution>.new(
      tools = [],
      tool_registry = registry,
      hooks = hooks,
      observers = [observer],
    );
    //# Run the live agent with dynamic capabilities
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, []);
    let outcome = task.run(
      runner = runner,
    );
    //# Inspect the provider conversation and emitted events
    match (outcome) {
      let done: root.ai.Done<Resolution> => match (done.conversation) {
        let conversation: root.ai.OpenAiConversation => assert.equal(conversation.tools_json.length(), 2),
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
    let handoff = root.ai.tool(lookup_account).as_handoff();
    //# Run until the handoff boundary
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [handoff]);
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(tools = [handoff]),
    );
    //# Verify control was transferred
    match (outcome) {
      let value: root.ai.Handoff => assert.equal(value.to, "lookup_account"),
      _ => assert.is_true(false),
    }
  }
}
