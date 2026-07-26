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
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            stream: root.ai.AudioStream,
        ) -> root.ai.Response<string> throws root.ai.CallError | baml.errors.UnknownError {
            match (self.provider) {
                let capability: root.ai.TranscriptionProvider => {
                    capability.transcribe(stream, self.options)
                },
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot transcribe audio",
        }
                },
            }
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
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            stream: root.ai.AudioStream,
        ) -> string throws root.ai.CallError | baml.errors.UnknownError {
            match (self.provider) {
                let capability: root.ai.TranscriptionProvider => {
                    capability.transcribe(stream, self.options).value
                },
                _ => {
                    throw baml.errors.Unsupported {
          message: "provider cannot transcribe audio",
        }
                },
            }
        }
    }
}
