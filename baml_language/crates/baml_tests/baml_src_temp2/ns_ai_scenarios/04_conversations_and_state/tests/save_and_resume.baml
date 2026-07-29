// Tests for the save_and_resume scenario.
test "guide 04.05: OpenAI seals and restores its exact provider conversation" {
  //# Begin a native provider conversation
  let provider = root.openai.Responses {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let conversation = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]),
  );
  //# Save and restore provider-owned wire state
  let token = provider.save_conversation(conversation);
  let restored = provider.restore_conversation(token);
  //# Verify restored provider ownership
  assert.equal(restored.provider().name(), "openai")
}

test "guide 04.05: an agent resumes the supplied conversation instead of starting over" {
  //# Advance a provider conversation through its first turn
  let provider = fake_tool_provider([one_knowledge_call()]);
  let task = ResolveTicketWithTools_task(sample_ticket(), provider, [search_knowledge]);
  let conversation = provider.begin<Resolution>(task);
  let tools = root.ai.tools.normalize_tools([search_knowledge]);
  let first = provider.step<Resolution>(conversation, tools);
  let calls = match (first.outcome) {
    let value: root.ai.tools.ToolCalls => value,
    let value: Resolution => baml.sys.panic("expected first tool turn"),
  };
  //# Submit tool results before handing control to the agent
  let results = calls.calls.map((call: root.ai.tools.ToolCall) -> root.ai.tools.ToolResult {
    root.ai.tools.invoke_tool(tools[0], call)
  });
  let resumed = provider.submit(conversation, results);
  let dispatched: string[] = [];
  //# Resume the agent from existing continuation state
  let outcome = task.run(
    runner = root.ai.run.Agent<Resolution>.new(
      tools = [search_knowledge],
      after_tool_call = (event) -> {
        dispatched.push(event.call.name);
        null
      },
      conversation = resumed,
    ),
  );
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
  //# Verify the completed tool call was not replayed
  assert.equal(dispatched.length(), 0)
}

test "guide 04.05: conversation tokens validate provider ownership" {
  //# Construct a token owned by another provider
  let provider = root.openai.Responses {
    ...root.openai.responses(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let wrong = root.ai.ConversationToken { provider: "anthropic", version: 1, sealed: "{}" };
  //# Reject restoration under the wrong provider
  let label = provider.restore_conversation(wrong) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}

testset "integ-test-guide-04-save-and-resume" {
  test "guide 04.05 live OpenAI: save after a tool request and resume to completion" {
    let provider = live_openai();
    let task = ToolInstructions_task(
      provider,
      [search_knowledge],
      `Call search_knowledge exactly once with query "duplicate charge". After its result, return the final resolution.`,
    );
    let conversation = provider.begin<Resolution>(task);
    let tools = root.ai.tools.normalize_tools([search_knowledge]);
    let first = provider.step<Resolution>(conversation, tools);
    let calls = match (first.outcome) {
      let value: root.ai.tools.ToolCalls => value,
      let value: Resolution => baml.sys.panic("expected a live tool request before save"),
    };
    let token = provider.save_conversation(conversation);
    let restored = provider.restore_conversation(token);
    let results = calls.calls.map((call: root.ai.tools.ToolCall) -> root.ai.tools.ToolResult {
      root.ai.tools.invoke_tool(tools[0], call)
    });
    let resumed = provider
      .as<root.ai.tools.ToolCallingProvider>
      .submit(restored, results);
    let dispatched: string[] = [];
    let outcome = task.run(
      runner = root.ai.run.Agent<Resolution>.new(
        tools = [search_knowledge],
        after_tool_call = (event) -> {
          dispatched.push(event.call.name);
          null
        },
        conversation = resumed,
      ),
    );
    log.info("live conversation resume " + {
      "saved_provider": token.provider,
      "saved_bytes": token.sealed.length(),
      "pre_resume_calls": calls.calls.map((call: root.ai.tools.ToolCall) -> string { call.name }),
      "replayed_dispatches": dispatched,
    }.to_string());
    assert.equal(token.provider, "openai")
    assert.equal(dispatched.length(), 0)
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
