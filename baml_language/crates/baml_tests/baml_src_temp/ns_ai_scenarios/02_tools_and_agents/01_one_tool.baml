test "guide 02.01: one tool call is dispatched and the model finishes" {
  let provider = fake_tool_provider([one_knowledge_call()]);
  let outcome = root.ai.drivers.run_agent<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
    guide_agent_options([knowledge_tool()], guide_dispatch),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
}

test "guide 02.01: prompt fallback extends ctx.output_format with active tool schemas" {
  let inner = root.ai.fake(`{
    "calls": [{
      "id": "call-1",
      "name": "search_knowledge",
      "args": {"query": "duplicate charge"}
    }]
  }`);
  let provider = root.ai.PromptToolProvider { inner: inner };
  let transcript = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
  );
  let state = match (transcript) {
    let value: root.ai.PromptToolTranscript => value,
    _ => baml.sys.panic("expected prompt tool transcript"),
  };
  let rendered = baml.llm.prompt_to_text(root.ai.prompt_tool_turn<Resolution | root.ai.ToolCalls>(
    provider,
    state,
    [knowledge_tool()],
    root.ai.prompt_tool_output_suffix([knowledge_tool()]),
  ).prompt);
  let turn = provider.step<Resolution>(transcript, [knowledge_tool()]);
  match (turn) {
    let calls: root.ai.ToolCalls => assert.equal(calls.calls[0].name, "search_knowledge"),
    _ => assert.is_true(false),
  }
  assert.contains(rendered, "calls")
  assert.contains(rendered, "Available tools:")
  assert.contains(rendered, "search_knowledge")
  assert.contains(rendered, "query")
}

testset "integ-test-guide-02-one-tool" {
  test "guide 02.01 live OpenAI: tool call and final typed value" {
    let provider = root.ai.OpenAi {
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
      guide_agent_options([knowledge_tool()], guide_dispatch),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Anthropic: prompt and SAP tool fallback" {
    let provider = root.ai.Anthropic {
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      base_url: null,
      extra_headers: null,
      extra_body: null,
    };
    let outcome = root.ai.drivers.run_agent<Resolution>(
      ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
      guide_agent_options([knowledge_tool()], guide_dispatch),
    );
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
