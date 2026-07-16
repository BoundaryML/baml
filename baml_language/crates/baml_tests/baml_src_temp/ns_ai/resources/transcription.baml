class TranscriptionOptions {
  language: string?,
  prompt: string?,
}

interface TranscriptionProvider requires Provider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> Response<string>
    throws baml.errors.CallError | baml.errors.UnknownError
}

class FakeTranscriptionProvider {
  output: string,
  received_chunks: audio[],
}

implements Provider for FakeTranscriptionProvider {
  function name(self) -> string throws never { "fake-transcription" }
}

implements TranscriptionProvider for FakeTranscriptionProvider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> Response<string>
      throws never {
    self.received_chunks = stream.collect();
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
      transcript: null,
    }
  }
}
