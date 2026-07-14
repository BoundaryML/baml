test "guide 05.02: AudioStream means finite input, not a live channel" {
  let stream = root.ai.AudioStream {
    chunks: [audio.from_url("https://example.com/one.wav", "audio/wav")],
    sample_rate_hz: 24000,
    channels: 1,
  };
  let task = TranscribeAudio_task(stream, root.ai.fake(`"hello"`));
  assert.equal(task.arguments["chunks"], 1)
  assert.equal(root.ai.drivers.drive<string>(task), "hello")
}
