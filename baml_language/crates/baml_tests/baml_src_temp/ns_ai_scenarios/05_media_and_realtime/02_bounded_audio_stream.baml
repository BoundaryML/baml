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

testset "integ-test-guide-05-bounded-audio" {
  test "guide 05.02 live OpenAI: one finite WAV is transcribed with bounded metadata" {
    let stream = root.ai.AudioStream {
      chunks: [audio.from_url(
        "https://raw.githubusercontent.com/openai/openai-dotnet/refs/heads/main/examples/Assets/realtime_whats_the_weather_pcm16_24khz_mono.wav",
        "audio/wav",
      )],
      sample_rate_hz: 24000,
      channels: 1,
    };
    let provider = root.ai.OpenAiAudioTranscription {
      inner: root.ai.OpenAi {
        ...live_openai(),
        model: "gpt-audio",
      },
      received_chunks: 0,
    };
    let response = root.ai.drivers.transcribe_with_meta(
      provider,
      stream,
      root.ai.TranscriptionOptions { language: "en", prompt: "a short weather question" },
    );
    log.info(baml.json.stringify({
      "guide": "05.02",
      "provider": response.meta.provider,
      "received_chunks": provider.received_chunks,
      "sample_rate_hz": stream.sample_rate_hz,
      "transcript_length": response.value.length(),
    }));
    assert.equal(provider.received_chunks, 1)
    assert.equal(response.meta.attributes["chunks"], 1)
    assert.equal(response.meta.attributes["sample_rate_hz"], 24000)
    assert.contains(response.value.to_lower_case(), "weather")
  }
}
