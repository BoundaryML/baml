test "guide 02.01: one tool call is dispatched and the model finishes" {
  //# Configure a provider with one tool call
  let provider = fake_tool_provider([one_knowledge_call()]);
  //# Run the tool-enabled agent
  let task = ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]);
  let outcome = task.drive(
    root.ai.driver.agent<Resolution>(
      root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch),
    ),
  );
  //# Verify typed completion
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
}

test "guide 02.01: prompt fallback extends ctx.output_format with active tool schemas" {
  //# Configure a generation-only provider
  let inner = root.ai.fake_output_provider(`{
    "calls": [{
      "id": "call-1",
      "name": "search_knowledge",
      "args": {"query": "duplicate charge"}
    }]
  }`);
  //# Begin a prompt-based tool transcript
  let provider = root.ai.PromptToolProvider { inner: inner };
  let transcript = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
  );
  let state = match (transcript) {
    let value: root.ai.PromptToolTranscript => value,
    _ => baml.sys.panic("expected prompt tool transcript"),
  };
  //# Render active tools into the output contract
  let rendered = baml.llm.prompt_to_text(root.ai.prompt_tool_turn<Resolution | root.ai.ToolCalls>(
    provider,
    state,
    [knowledge_tool()],
    root.ai.prompt_tool_output_suffix([knowledge_tool()]),
  ).prompt);
  //# Execute one prompt-tool turn
  let turn = provider.step<Resolution>(transcript, [knowledge_tool()]);
  match (turn.outcome) {
    let calls: root.ai.ToolCalls => assert.equal(calls.calls[0].name, "search_knowledge"),
    let value: Resolution => assert.is_true(false),
  }
  //# Verify the rendered tool schema
  assert.contains(rendered, "calls")
  assert.contains(rendered, "Available tools:")
  assert.contains(rendered, "search_knowledge")
  assert.contains(rendered, "query")
}

test "guide 02.01: ModelStep preserves a union task result" {
  //# Build a task whose final value is already a union
  let inner = root.ai.fake_output_provider(`"done"`);
  let provider = root.ai.PromptToolProvider { inner: inner };
  let task = root.ai.task<Resolution | string>(
    provider,
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        ["Return done.", ctx.output_format],
      )
    },
  );
  //# Execute through the prompt-tool adapter
  let transcript = provider.begin<Resolution | string>(task);
  let turn = provider.step<Resolution | string>(transcript, [knowledge_tool()]);
  match (turn.outcome) {
    let value: string => assert.equal(value, "done"),
    let value: Resolution => assert.is_true(false),
    let calls: root.ai.ToolCalls => assert.is_true(false),
  }
}

testset "integ-test-guide-02-one-tool" {
  test "guide 02.01 live OpenAI: tool call and final typed value" {
    //# Configure native OpenAI tool calling
    let provider = root.ai.OpenAi {
      ...root.ai.openai(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    //# Run the live tool loop
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]);
    let outcome = task.drive(
      root.ai.driver.agent<Resolution>(
        root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch),
      ),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Anthropic: prompt and SAP tool fallback" {
    //# Configure a generation provider for prompt-tool adaptation
    let provider = root.ai.Anthropic {
      ...root.ai.anthropic(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    };
    //# Run the live prompt-tool loop
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]);
    let outcome = task.drive(
      root.ai.driver.agent<Resolution>(
        root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch),
      ),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
