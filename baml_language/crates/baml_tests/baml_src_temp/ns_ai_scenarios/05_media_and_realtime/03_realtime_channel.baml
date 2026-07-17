test "guide 05.03: a realtime-only provider opens a channel instead of returning T" {
  //# Configure a realtime provider and recording channel
  let provider: root.ai.Provider = root.ai.FakeRealtimeProvider { transcript: "pong" };
  let channel = root.ai.RecordingChannel { frames: [] };
  //# Open the live interaction
  let live = root.ai.drivers.open_live<string>(VoiceSupport_task("reply pong", provider), channel);
  //# Collect transcript events and frames
  assert.equal(root.ai.collect_live_text(live), "pong")
  assert.is_true(channel.frames.length() > 0)
}

testset "integ-test-guide-05-realtime" {
  test "guide 05.03 live OpenAI: realtime WebSocket text exchange" {
    //# Configure a recording channel
    let channel = root.ai.RecordingChannel { frames: [] };
    //# Open a live OpenAI WebSocket interaction
    let live = root.ai.drivers.open_live<string>(
      VoiceSupport_task("Reply with exactly: pong", root.ai.OpenAiRealtime {
        model: "gpt-realtime",
        voice: "alloy",
        api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
        output_audio: false,
      }),
      channel,
    );
    //# Collect and verify realtime transcript deltas
    let transcript = root.ai.collect_live_text(live);
    log.info(baml.json.stringify({
      "guide": "05.03",
      "provider_frames": channel.frames.length(),
      "transcript_length": transcript.length(),
    }));
    assert.contains(transcript.to_lower_case(), "pong")
    match (live) {
      let session: root.ai.OpenAiRealtimeLive => {
        assert.is_true(session.is_closed)
        assert.is_true(session.items.length() > 0)
      },
      _ => assert.is_true(false),
    }
  }
}
