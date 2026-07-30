// Tests for the bounded_audio_stream scenario.
test "guide 05.02: AudioStream means finite input, not a live channel" {
    let stream = ai.transcription.AudioStream {
        chunks: [audio.from_url("https://example.com/one.wav", "audio/wav")],
        sample_rate_hz: 24000,
        channels: 1,
    };
    let provider = ai.testing.FakeTranscriptionProvider {
        output: "hello",
        received_chunks: [],
        received_options: null,
    };
    //# Execute the transcription task with metadata-preserving policy
    let response = TranscribeCall_task(stream, provider)
        .run(runner = ai.run.TranscribeWithMeta.new(language = "en"));
    assert.equal(response.value, "hello");
    assert.equal(provider.received_chunks.length(), 1);
    assert
        .equal(
            provider.received_options?.prompt,
            "Transcribe this finite customer-support recording.",
        );
    assert.equal(response.metadata.attributes["sample_rate_hz"], 24000)
}
