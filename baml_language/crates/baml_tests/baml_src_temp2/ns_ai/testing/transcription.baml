class FakeTranscriptionProvider {
    output: string,
    received_chunks: audio[],
    received_options: TranscriptionOptions?,
}

implements Provider for FakeTranscriptionProvider {
  function name(self) -> string throws never { "fake-transcription" }
  function render_shorthand(self) -> string throws never { fake_render_shorthand() }
}

implements TranscriptionProvider for FakeTranscriptionProvider {
  function transcribe(
    self,
    stream: AudioStream,
    options: TranscriptionOptions,
  ) -> Response<string> throws never {
    self.received_chunks = stream.collect();
    self.received_options = options;
    Response<string> {
      value: self.output,
      meta: Meta {
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
