test "guide 05.05: realtime tasks retain the same tool roster" {
  let task = VoiceSupport_task("look up the account", root.ai.FakeRealtimeProvider { transcript: "done" });
  let with_tools = task.with_tools([account_tool()]);
  assert.equal(with_tools.tools[0].name, "lookup_account")
}

testset "integ-test-guide-05-realtime-tools" {
  test "guide 05.05 live OpenAI: tool result is spliced into the retained realtime session" {
    let provider = root.ai.OpenAiRealtime {
      model: "gpt-realtime",
      voice: "alloy",
      api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      output_audio: false,
    };
    let base = VoiceSupport_task(
      "Call lookup_account with customer_id C-1. Then report the returned status and tier.",
      provider,
    );
    let task = root.ai.Task<null> {
      ...base,
      tools: [account_tool()],
      options: root.ai.TaskOptions {
        ...base.options,
        dispatch: guide_dispatch,
      },
    };
    let channel = root.ai.RecordingChannel { frames: [] };
    let live = task.drive(root.ai.driver.live(channel));
    let requested = live.events();
    let calls: root.ai.ToolCall[] = [];
    for (let event in requested) {
      match (event) {
        let tool_event: root.ai.LiveToolCalls => {
          for (let call in tool_event.calls) { calls.push(call); }
        },
        _ => {},
      }
    }
    assert.equal(calls.length(), 1)
    assert.equal(calls[0].name, "lookup_account")
    assert.equal(baml.json.path_or(calls[0].args, ".customer_id", ""), "C-1")
    let results = guide_dispatch(calls);
    assert.equal(results.length(), 1)
    assert.is_true(!results[0].is_error)
    live.submit_tool_results(results);
    let transcript = root.ai.collect_live_text(live);
    log.info(baml.json.stringify({
      "guide": "05.05",
      "tool_name": calls[0].name,
      "tool_call_id": calls[0].id,
      "submitted_results": results.length(),
      "transcript_length": transcript.length(),
    }));
    assert.contains(transcript.to_lower_case(), "active")
    assert.contains(transcript.to_lower_case(), "pro")
    match (live) {
      let session: root.ai.OpenAiRealtimeLive => {
        assert.equal(session.submitted_results.length(), 1)
        assert.equal(session.submitted_results[0].id, calls[0].id)
      },
      _ => assert.is_true(false),
    }
  }
}
