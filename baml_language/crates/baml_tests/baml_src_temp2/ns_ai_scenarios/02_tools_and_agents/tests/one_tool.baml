test "guide 02.01: one tool call is dispatched and the model finishes" {
  //# Configure a provider with one tool call
  let provider = fake_tool_provider([one_knowledge_call()]);
  //# Run the tool-enabled agent
  let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
  let outcome = task.run(
    runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
  );
  //# Verify the typed final value
  match (outcome) {
    let done: ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
}

test "guide 02.01: OpenAI prompt mode extends ctx.output_format with active tool schemas" {
  //# Configure a provider-owned prompt-tool conversation
  let provider = root.openai.responses(
    api_key = "unused",
    output_mode = root.openai.OutputMode.Sap,
    tool_mode = root.openai.ToolMode.Prompt,
  );
  let conversation = provider.begin<Resolution>(
    ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]),
  );
  let state = match (conversation) {
    let value: root.openai.internal.PromptConversation => value,
    _ => baml.sys.panic("expected OpenAI prompt-tool conversation"),
  };
  let tools = ai.tools.normalize_tools([search_knowledge]);
  //# Render active tools into the output contract
  let rendered = root.openai.internal._prompt_turn_task<Resolution | ai.tools.ToolCalls>(
    provider,
    state,
    root.openai.internal._prompt_tool_output_suffix(tools),
  ).prompt.text();
  //# Verify the rendered tool schema
  assert.contains(rendered, "calls")
  assert.contains(rendered, "Available tools:")
  assert.contains(rendered, "search_knowledge")
  assert.contains(rendered, "query")
}

test "guide 02.01: ModelStep preserves a union task result" {
  //# Build a task whose final value is already a union
  let provider = root.openai.responses(
    api_key = "unused",
    output_mode = root.openai.OutputMode.Sap,
    tool_mode = root.openai.ToolMode.Prompt,
  );
  let task = ai.task<Resolution | string>(
    provider,
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        ["Return done.", ctx.output_format],
      )
    },
  );
  //# Ask the provider to render T | ToolCalls without erasing T's union
  let conversation = provider.begin<Resolution | string>(task);
  let state = match (conversation) {
    let value: root.openai.internal.PromptConversation => value,
    _ => baml.sys.panic("expected OpenAI prompt-tool conversation"),
  };
  let tools = ai.tools.normalize_tools([search_knowledge]);
  let rendered = root.openai.internal._prompt_turn_task<
    Resolution | string | ai.tools.ToolCalls
  >(
    provider,
    state,
    root.openai.internal._prompt_tool_output_suffix(tools),
  );
  assert.contains(rendered.prompt.text(), "category");
  assert.contains(rendered.prompt.text(), "reply");
  assert.contains(rendered.prompt.text(), "string");
  assert.contains(rendered.prompt.text(), "calls")
}

testset "integ-test-guide-02-one-tool" {
  test "guide 02.01 live OpenAI: tool call and final typed value" {
    //# Configure native OpenAI tool calling
    let provider = root.openai.OpenAIProvider {
      ...root.openai.responses(),
      model: "gpt-5.6-luna",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
    };
    //# Run the live tool loop
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(
      runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Anthropic: native tools and strict output" {
    //# Configure native Anthropic tool calling with structured output
    let provider = root.anthropic.AnthropicProvider {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
    };
    let trace = guide_agent_trace();
    let task = ToolInstructions@task(`Call search_knowledge exactly once with query "duplicate charge". After its result, return a final resolution with a non-empty summary and reply.`).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(
      runner = ai.run.Agent<Resolution>.new(
        tools = [search_knowledge],
        after_tool_call = (event) -> {
          trace.after_calls.push(event.call.name);
          null
        },
      ),
    );
    assert.equal(trace.after_calls, ["search_knowledge"]);
    match (outcome) {
      let done: ai.Done<Resolution> => {
        assert.is_true(done.value.summary.length() > 0);
        assert.is_true(done.value.reply.length() > 0)
      },
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Anthropic: prompt and SAP tool fallback" {
    //# Configure Anthropic's provider-owned prompt-tool mode
    let provider = root.anthropic.AnthropicProvider {
      ...root.anthropic.messages(),
      model: "claude-sonnet-4-6",
      api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
      output_mode: root.anthropic.OutputMode.Sap,
      tool_mode: root.anthropic.ToolMode.Prompt,
    };
    //# Run the live prompt-tool loop
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(
      runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    //# Verify the live typed result
    match (outcome) {
      let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Google AI: native tools and typed output" {
    let provider = root.google.ai.gemini(
      api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
      output_mode = root.google.ai.OutputMode.Native,
      tool_mode = root.google.ai.ToolMode.Native,
    );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(
      runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    match (outcome) {
      let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }

  test "guide 02.01 live Google AI: prompt and SAP tool fallback" {
    let provider = root.google.ai.gemini(
      api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
      output_mode = root.google.ai.OutputMode.Sap,
      tool_mode = root.google.ai.ToolMode.Prompt,
    );
    let task = ResolveTicketWithTools@task(sample_ticket()).with_provider(provider).with_tools([search_knowledge]);
    let outcome = task.run(
      runner = ai.run.Agent<Resolution>.new(tools = [search_knowledge]),
    );
    match (outcome) {
      let done: ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
