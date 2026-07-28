// Tests for the streaming_failure_boundaries scenario.
test "guide 03.07: a generation retry wrapper does not invent stream replay" {
  let provider: root.ai.Provider = root.ai.retry(fake_resolution(), 2);
  let capability = match (provider) {
    let streaming: root.ai.StreamingProvider => "streaming",
    _ => "not-streaming",
  };
  assert.equal(capability, "not-streaming")
}

testset "integ-test-guide-03-streaming-failure-boundary" {
  test "guide 03.07 live OpenAI: streaming works directly but retry does not claim it" {
    let provider = live_openai();
    let value = drain_resolution_stream(ResolveTicket_task(sample_ticket(), provider));
    let wrapped: root.ai.Provider = root.ai.retry(provider, 2);
    let capability = match (wrapped) {
      let streaming: root.ai.StreamingProvider => "streaming",
      _ => "not-streaming",
    };
    log.info("live stream replay boundary " + {
      "direct_stream_reply": value.reply,
      "wrapped_capability": capability,
    }.to_string());
    assert.is_true(value.reply.length() > 0)
    assert.equal(capability, "not-streaming")
  }
}
