test "guide 05.04: interruption belongs to the opened live resource" {
  let live = root.ai.drivers.open_live<string>(
    VoiceSupport_task("talk", root.ai.FakeRealtimeProvider { transcript: "hello" }),
    root.ai.RecordingChannel { frames: [] },
  );
  live.cancel_response();
  live.truncate_assistant_audio(250);
  live.close();
  assert.equal(root.ai.collect_live_text(live), "hello")
}

