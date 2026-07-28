class TranscriptionOptions {
    language: string?,
    prompt: string?,
}

interface TranscriptionProvider requires Provider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> ResponseWithMetadata<string>
    throws root.ai.CallError | baml.errors.UnknownError
}

// Executable prototype for the function-specific task type that
// TranscribeAudio.task(audio) will construct. Keeping the audio typed prevents
// unrelated Task<string> values from being accepted by a transcription runner.
class TranscriptionTask {
    provider: Provider,
    audio: AudioStream,
    prompt: string,
    identity: TaskIdentity?,

    function run<Output, Error, R extends Runner<TranscriptionTask, Output = Output, Error = Error>>(
        self,
        runner: R,
    ) -> Output throws Error {
        //# Dispatch this finite-audio task to the selected transcription policy
        runner.run(self)
    }
}
