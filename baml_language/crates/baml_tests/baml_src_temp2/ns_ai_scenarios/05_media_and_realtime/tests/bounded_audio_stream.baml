// Tests for the bounded_audio_stream scenario.
test "guide 05.02: AudioStream means finite input, not a live channel" {
  let stream = root.ai.AudioStream {
    chunks: [audio.from_url("https://example.com/one.wav", "audio/wav")],
    sample_rate_hz: 24000,
    channels: 1,
  };
  let provider = root.ai.FakeTranscriptionProvider {
    output: "hello",
    received_chunks: [],
    received_options: null,
  };
  //# Execute the transcription task with metadata-preserving policy
  let response = TranscribeCall_task(stream, provider).run(
    runner = root.ai.run.TranscribeWithMeta.new(language = "en"),
  );
  assert.equal(response.value, "hello")
  assert.equal(provider.received_chunks.length(), 1)
  assert.equal(
    provider.received_options?.prompt,
    "Transcribe this finite customer-support recording.",
  )
  assert.equal(response.meta.attributes["sample_rate_hz"], 24000)
}

testset "integ-test-guide-05-bounded-audio" {
  test "guide 05.02 live OpenAI: one finite WAV is transcribed with bounded metadata" {
    //# Fetch the WAV before task construction because OpenAI audio input is
    //# encoded inline; an arbitrary remote audio URL is not an input modality.
    let wav = baml.http.fetch(
      "https://raw.githubusercontent.com/openai/openai-dotnet/refs/heads/main/examples/Assets/realtime_whats_the_weather_pcm16_24khz_mono.wav",
    );
    assert.is_true(wav.ok())
    let stream = root.ai.AudioStream {
      chunks: [audio.from_base64(
        wav.bytes().to_base64(),
        "audio/wav",
      )],
      sample_rate_hz: 24000,
      channels: 1,
    };
    let provider = root.openai.AudioTranscription {
      inner: root.openai.Chat {
        ...live_openai(),
        model: "gpt-audio",
      },
      received_chunks: 0,
    };
    //# Keep provider and audio on the task; configure only policy on the runner
    let response = TranscribeCall_task(stream, provider).run(
      runner = root.ai.run.TranscribeWithMeta.new(
        language = "en",
        prompt = "a short weather question",
      ),
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
