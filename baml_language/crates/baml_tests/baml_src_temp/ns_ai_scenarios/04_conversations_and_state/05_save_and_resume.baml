test "guide 04.05: OpenAI seals and restores its exact wire transcript" {
  //# Begin a native provider transcript
  let provider = root.ai.OpenAi {
    ...root.ai.openai(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let transcript = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
  );
  //# Save and restore provider-owned wire state
  let token = provider.save_transcript(transcript);
  let restored = provider.restore_transcript(token);
  //# Verify restored provider ownership
  assert.equal(restored.provider().name(), "openai")
}

test "guide 04.05: an agent resumes the supplied transcript instead of starting over" {
  //# Advance a tool transcript through its first turn
  let provider = fake_tool_provider([one_knowledge_call()]);
  let task = ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]);
  let transcript = provider.begin<Resolution>(task);
  let first = provider.step<Resolution>(transcript, [knowledge_tool()]);
  let calls = match (first.outcome) {
    let value: root.ai.ToolCalls => value,
    let value: Resolution => baml.sys.panic("expected first tool turn"),
  };
  //# Submit tool results before handing control to the agent
  let resumed = provider.submit(transcript, guide_dispatch(calls.calls));
  let hooks = no_op_hooks();
  let options = guide_agent_options([knowledge_tool()], guide_dispatch, hooks = hooks);
  //# Resume the agent from existing continuation state
  let outcome = root.ai.drivers.run_agent<Resolution>(task.with_transcript(resumed), options);
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
  //# Verify the completed tool call was not replayed
  assert.equal(hooks.after_calls.length(), 0)
}

test "guide 04.05: transcript tokens validate provider ownership" {
  //# Construct a token owned by another provider
  let provider = root.ai.OpenAi {
    ...root.ai.openai(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let wrong = root.ai.TranscriptToken { provider: "anthropic", version: 1, sealed: "{}" };
  //# Reject restoration under the wrong provider
  let label = provider.restore_transcript(wrong) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}
