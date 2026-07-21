// Bounded transcription is not prompt-shaped Task<T> execution. The driver
// captures the selected provider and options, then consumes one finite stream.
class TranscribeWithMeta {
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,
}

implements root.ai.Driver<root.ai.AudioStream> for TranscribeWithMeta {
  type Output = root.ai.Response<string>
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, stream: root.ai.AudioStream) -> root.ai.Response<string>
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.transcribe_with_meta(self.provider, stream, self.options)
  }
}

function transcribe_with_meta(
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,
) -> TranscribeWithMeta throws never {
  TranscribeWithMeta { provider: provider, options: options }
}

class Transcribe {
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,
}

implements root.ai.Driver<root.ai.AudioStream> for Transcribe {
  type Output = string
  type Error = baml.errors.CallError | baml.errors.UnknownError

  function drive(self, stream: root.ai.AudioStream) -> string
      throws baml.errors.CallError | baml.errors.UnknownError {
    root.ai.drivers.transcribe(self.provider, stream, self.options)
  }
}

function transcribe(
  provider: root.ai.Provider,
  options: root.ai.TranscriptionOptions,
) -> Transcribe throws never {
  Transcribe { provider: provider, options: options }
}
