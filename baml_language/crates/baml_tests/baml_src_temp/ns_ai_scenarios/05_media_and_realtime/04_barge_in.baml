test "guide 05.04: interruption belongs to the opened live resource" {
  let live = root.ai.drivers.open_live<string>(
    VoiceSupport_task("talk", root.ai.FakeRealtimeProvider { transcript: "hello" }),
    root.ai.RecordingChannel { frames: [] },
  );
  live.cancel_response();
  live.truncate_assistant_audio(250);
  live.cleanup();
  live.cleanup();
  assert.equal(root.ai.collect_live_text(live), "hello")
  match (live) {
    let buffered: root.ai.BufferedLive => assert.is_true(buffered.is_closed),
    _ => assert.is_true(false),
  }
}
