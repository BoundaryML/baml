test "guide 05.02: AudioStream means finite input, not a live channel" {
  let stream = root.ai.AudioStream {
    chunks: [audio.from_url("https://example.com/one.wav", "audio/wav")],
    sample_rate_hz: 24000,
    channels: 1,
  };
  let provider = root.ai.FakeTranscriptionProvider { output: "hello", received_chunks: [] };
  let response = root.ai.drivers.transcribe_with_meta(
    provider,
    stream,
    root.ai.TranscriptionOptions { language: "en", prompt: null },
  );
  assert.equal(response.value, "hello")
  assert.equal(provider.received_chunks.length(), 1)
  assert.equal(response.meta.attributes["sample_rate_hz"], 24000)
}
