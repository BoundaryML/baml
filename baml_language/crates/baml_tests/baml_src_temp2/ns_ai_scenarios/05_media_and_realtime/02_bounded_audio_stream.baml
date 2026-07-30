// Transcribe a finite audio recording while retaining metadata about the
// provider response.
function bounded_audio_stream() -> ai.ResponseWithMetadata<string> {
    let stream = ai.transcription.AudioStream {
        chunks: [audio.from_url("https://example.com/one.wav", "audio/wav")],
        sample_rate_hz: 24000,
        channels: 1,
    };
    let provider = ai.testing.FakeTranscriptionProvider {
        output: "hello from a finite audio stream",
        received_chunks: [],
        received_options: null,
    };
    let response = TranscribeCall_task(stream, provider)
        .run(runner = ai.run.TranscribeWithMeta.new(language = "en"));
    log.info(
        {
            "scenario": "bounded_audio_stream",
            "received_chunks": provider.received_chunks.length(),
            "response": response,
        },
    );
    response
}
