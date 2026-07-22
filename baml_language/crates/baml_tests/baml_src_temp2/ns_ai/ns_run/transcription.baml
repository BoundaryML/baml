// Bounded transcription is not prompt-shaped Task<T> execution. The runner
// captures the selected provider and options, then consumes one finite stream.
class TranscribeWithMeta {
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,

  function new(
    provider: root.ai.Provider,
    language: string? = null,
    prompt: string? = null,
  ) -> TranscribeWithMeta throws never {
    TranscribeWithMeta {
      provider: provider,
      options: root.ai.TranscriptionOptions { language: language, prompt: prompt },
    }
  }

  implements root.ai.Runner<root.ai.AudioStream> {
    type Output = root.ai.Response<string>
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, stream: root.ai.AudioStream) -> root.ai.Response<string>
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.transcribe_with_meta(self.provider, stream, self.options)
    }
  }
}

class Transcribe {
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,

  function new(
    provider: root.ai.Provider,
    language: string? = null,
    prompt: string? = null,
  ) -> Transcribe throws never {
    Transcribe {
      provider: provider,
      options: root.ai.TranscriptionOptions { language: language, prompt: prompt },
    }
  }

  implements root.ai.Runner<root.ai.AudioStream> {
    type Output = string
    type Error = baml.errors.CallError | baml.errors.UnknownError

    function run(self, stream: root.ai.AudioStream) -> string
        throws baml.errors.CallError | baml.errors.UnknownError {
      root.ai.drivers.transcribe(self.provider, stream, self.options)
    }
  }
}
