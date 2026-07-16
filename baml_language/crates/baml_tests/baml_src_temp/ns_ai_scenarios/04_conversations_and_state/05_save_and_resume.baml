test "guide 04.05: OpenAI seals and restores its exact wire transcript" {
  let provider = root.ai.OpenAi {
    ...root.ai.openai(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let transcript = provider.begin<Resolution>(
    ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]),
  );
  let token = provider.save_transcript(transcript);
  let restored = provider.restore_transcript(token);
  assert.equal(restored.provider().name(), "openai")
}

test "guide 04.05: an agent resumes the supplied transcript instead of starting over" {
  let provider = fake_tool_provider([one_knowledge_call()]);
  let task = ResolveTicketWithTools_task(sample_ticket(), provider, [knowledge_tool()]);
  let transcript = provider.begin<Resolution>(task);
  let first = provider.step<Resolution>(transcript, [knowledge_tool()]);
  let calls = match (first.outcome) {
    let value: root.ai.ToolCalls => value,
    let value: Resolution => baml.sys.panic("expected first tool turn"),
  };
  let resumed = provider.submit(transcript, guide_dispatch(calls.calls));
  let hooks = no_op_hooks();
  let options = guide_agent_options([knowledge_tool()], guide_dispatch, hooks = hooks);
  let outcome = root.ai.drivers.run_agent<Resolution>(task.with_transcript(resumed), options);
  match (outcome) {
    let done: root.ai.Done<Resolution> => assert.equal(done.value.category, "billing"),
    _ => assert.is_true(false),
  }
  assert.equal(hooks.after_calls.length(), 0)
}

test "guide 04.05: transcript tokens validate provider ownership" {
  let provider = root.ai.OpenAi {
    ...root.ai.openai(),
    model: "gpt-5.6-luna",
    api_key: "unused",
  };
  let wrong = root.ai.TranscriptToken { provider: "anthropic", version: 1, sealed: "{}" };
  let label = provider.restore_transcript(wrong) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}
