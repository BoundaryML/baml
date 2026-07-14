testset "integ-test-guide-02-agent-capabilities" {
  test "guide 02 live OpenAI: dynamic tools, hooks, events, and parallel-capable dispatch" {
    let provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null, extra_headers: null, extra_body: null,
    };
    let registry = root.ai.ToolRegistry.new([knowledge_tool()]);
    let hooks = no_op_hooks();
    hooks.registry = registry;
    hooks.tool_to_add = account_tool();
    let observer = GuideObserver { kinds: [] };
    let options = guide_agent_options([], guide_dispatch);
    options.tool_registry = registry;
    options.hooks = hooks;
    options.observers = [observer];
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, []),
      options,
    );
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
    let provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null, extra_headers: null, extra_body: null,
    };
    let handoff = account_tool().as_handoff();
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [handoff]),
      guide_agent_options([handoff], guide_dispatch),
    );
    match (outcome) {
      let value: root.ai.Handoff => assert.equal(value.to, "lookup_account"),
      _ => assert.is_true(false),
    }
  }
}

