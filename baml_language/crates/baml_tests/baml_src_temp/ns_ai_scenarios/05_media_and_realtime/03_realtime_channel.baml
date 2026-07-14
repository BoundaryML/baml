test "guide 05.03: a realtime-only provider opens a channel instead of returning T" {
  let provider: root.ai.Provider = root.ai.FakeRealtimeProvider { transcript: "pong" };
  let channel = root.ai.RecordingChannel { frames: [] };
  let live = root.ai.drivers.open_live<string>(VoiceSupport_task("reply pong", provider), channel);
  assert.equal(root.ai.collect_live_text(live), "pong")
  assert.is_true(channel.frames.length() > 0)
}

testset "integ-test-guide-05-realtime" {
  test "guide 05.03 live OpenAI: realtime WebSocket text exchange" {
    let channel = root.ai.RecordingChannel { frames: [] };
    let live = root.ai.drivers.open_live<string>(
      VoiceSupport_task("Reply with exactly: pong", root.ai.OpenAiRealtime {
        model: "gpt-realtime",
        voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
      }),
      channel,
    );
    assert.contains(root.ai.collect_live_text(live).to_lower_case(), "pong")
  }
}

