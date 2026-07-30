class TranscriptionOptions {
    language: string?,
    prompt: string?,
}

interface TranscriptionProvider requires ai.Provider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> ai.ResponseWithMetadata<string>
    throws ai.Failure | baml.errors.UnknownError
}

// A function-specific finite-audio task. Keeping the audio typed prevents an
// unrelated Task<string> from being accepted by a transcription runner.
class TranscriptionTask {
    provider: ai.Provider,
    audio: AudioStream,
    prompt: string,
    identity: ai.TaskIdentity?,

    function run<Output, Error, R extends ai.Runner<TranscriptionTask, Output = Output, Error = Error>>(
        self,
        runner: R,
    ) -> Output throws Error {
        //# Dispatch this finite-audio task to the selected transcription policy
        runner.run(self)
    }
}
