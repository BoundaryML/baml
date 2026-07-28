test "guide 02.01: one tool call is dispatched and the model finishes" {
  //# Configure a provider with one tool call
  let provider = fake_tool_provider([one_knowledge_call()]);
  //# Run the tool-enabled agent
  let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
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
  //# Begin a prompt-based provider conversation
  let provider = root.ai.PromptToolProvider { inner: inner };
  let conversation = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]),
  );
  let state = match (conversation) {
    let value: root.ai.PromptToolConversation => value,
    _ => baml.sys.panic("expected prompt tool conversation"),
  };
  let tools = root.ai.normalize_tools([search_knowledge]);
  //# Render active tools into the output contract
  let rendered = root.ai._prompt_tool_turn<Resolution | root.ai.ToolCalls>(
    provider,
    state,
    tools,
    root.ai._prompt_tool_output_suffix(tools),
  ).prompt.text();
  //# Execute one prompt-tool turn
  let turn = provider.step<Resolution>(conversation, tools);
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
  let conversation = provider.begin<Resolution | string>(task);
  let turn = provider.step<Resolution | string>(
    conversation,
    root.ai.normalize_tools([search_knowledge]),
  );
  match (turn.outcome) {
    let value: string => assert.equal(value, "done"),
    let value: Resolution => assert.is_true(false),
    let calls: root.ai.ToolCalls => assert.is_true(false),
  }
}

testset "integ-test-guide-02-one-tool" {
  test "guide 02.01 live OpenAI: tool call and final typed value" {
    //# Configure native OpenAI tool calling
    let provider = root.openai.Chat {
      ...root.openai.chat(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    //# Run the live tool loop
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Anthropic: prompt and SAP tool fallback" {
    //# Configure a generation provider for prompt-tool adaptation
    let provider = root.anthropic.Messages {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    };
    //# Run the live prompt-tool loop
    let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
