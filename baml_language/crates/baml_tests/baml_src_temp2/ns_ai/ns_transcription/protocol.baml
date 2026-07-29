class TranscriptionOptions {
    language: string?,
    prompt: string?,
}

interface TranscriptionProvider requires root.ai.Provider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> root.ai.ResponseWithMetadata<string>
    throws root.ai.Failure | baml.errors.UnknownError
}

// Executable prototype for the function-specific task type that
// TranscribeAudio.task(audio) will construct. Keeping the audio typed prevents
// unrelated Task<string> values from being accepted by a transcription runner.
class TranscriptionTask {
    provider: root.ai.Provider,
    audio: AudioStream,
    prompt: string,
    identity: root.ai.TaskIdentity?,

    function run<Output, Error, R extends root.ai.Runner<TranscriptionTask, Output = Output, Error = Error>>(
        self,
        runner: R,
    ) -> Output throws Error {
        //# Dispatch this finite-audio task to the selected transcription policy
        runner.run(self)
    }
}
