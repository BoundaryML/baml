class FakeTranscriptionProvider {
    output: string,
    received_chunks: audio[],
    received_options: root.ai.transcription.TranscriptionOptions?,
}

implements root.ai.Provider for FakeTranscriptionProvider {
  function name(self) -> string throws never { "fake-transcription" }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements root.ai.transcription.TranscriptionProvider for FakeTranscriptionProvider {
  function transcribe(
    self,
    stream: root.ai.transcription.AudioStream,
    options: root.ai.transcription.TranscriptionOptions,
  ) -> root.ai.ResponseWithMetadata<string> throws never {
    //# Drain the finite audio stream
    self.received_chunks = stream.collect();
    //# Retain the effective transcription options
    self.received_options = options;
    //# Return transcript text with audio metadata
    root.ai.ResponseWithMetadata<string> {
      value: self.output,
      metadata: root.ai.ResponseMetadata {
        provider: self.name(),
        model: "fake",
        request_id: null,
        finish_reason: "stop",
        usage: null,
        attributes: {
          "chunks": self.received_chunks.length(),
          "sample_rate_hz": stream.sample_rate_hz,
        },
        raw: null,
      },
      conversation: null,
    }
  }
}
