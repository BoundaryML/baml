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
  let token = root.ai.driver.save_transcript(transcript).drive(provider);
  let restored = root.ai.driver.restore_transcript(token).drive(provider);
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
  let options = root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch, hooks = hooks);
  //# Resume the agent from existing continuation state
  let outcome = task.with_transcript(resumed).drive(
    root.ai.driver.agent<Resolution>(options),
  );
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
  let label = root.ai.driver.restore_transcript(wrong).drive(provider) catch (e) {
    let unknown: baml.errors.UnknownError => "rejected",
  };
  assert.equal(label, "rejected")
}

testset "integ-test-guide-04-save-and-resume" {
  test "guide 04.05 live OpenAI: save after a tool request and resume to completion" {
    let provider = live_openai();
    let task = ToolInstructions_task(
      provider,
      [knowledge_tool()],
      `Call search_knowledge exactly once with query "duplicate charge". After its result, return the final resolution.`,
    );
    let transcript = provider.begin<Resolution>(task);
    let first = provider.step<Resolution>(transcript, [knowledge_tool()]);
    let calls = match (first.outcome) {
      let value: root.ai.ToolCalls => value,
      let value: Resolution => baml.sys.panic("expected a live tool request before save"),
    };
    let token = root.ai.driver.save_transcript(transcript).drive(provider);
    let restored = root.ai.driver.restore_transcript(token).drive(provider);
    let resumed = provider.submit(restored, guide_dispatch(calls.calls));
    let hooks = no_op_hooks();
    let options = root.ai.AgentOptions.new([knowledge_tool()], guide_dispatch, hooks = hooks);
    let outcome = task.with_transcript(resumed).drive(
      root.ai.driver.agent<Resolution>(options),
    );
    log.info("live transcript resume " + {
      "saved_provider": token.provider,
      "saved_bytes": token.sealed.length(),
      "pre_resume_calls": calls.calls.map((call: root.ai.ToolCall) -> string { call.name }),
      "replayed_dispatches": hooks.after_calls,
    }.to_string());
    assert.equal(token.provider, "openai")
    assert.equal(hooks.after_calls.length(), 0)
    match (outcome) {
      let done: root.ai.Done<Resolution> => assert.is_true(done.value.reply.length() > 0),
      _ => assert.is_true(false),
    }
  }
}
