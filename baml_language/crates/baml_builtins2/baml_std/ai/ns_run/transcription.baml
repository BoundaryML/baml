// Bounded transcription executes a typed task. The task owns the selected
// provider and finite audio argument; the runner owns only transcription
// policy.

function _transcription_options(
    task: ai.transcription.TranscriptionTask,
    configured: ai.transcription.TranscriptionOptions,
) -> ai.transcription.TranscriptionOptions throws never {
    //# Merge runner overrides with the task's transcription prompt
    ai.transcription.TranscriptionOptions {
        language: configured.language,
        prompt: configured.prompt ?? task.prompt,
    }
}

class TranscribeWithMeta {
    options: ai.transcription.TranscriptionOptions,

    function new(
        language: string? = null,
        prompt: string? = null,
    ) -> TranscribeWithMeta throws never {
        TranscribeWithMeta {
            options: ai.transcription.TranscriptionOptions { language: language, prompt: prompt },
        }
    }

    implements ai.Runner<ai.transcription.TranscriptionTask> {
        type Output = ai.ResponseWithMetadata<string>
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.transcription.TranscriptionTask,
        ) -> ai.ResponseWithMetadata<string> throws ai.Failure | baml.errors.UnknownError {
            //# Select bounded-audio transcription and retain provider metadata
            match (task.provider) {
                let capability: ai.transcription.TranscriptionProvider => {
                    //## Transcribe the finite audio stream
                    capability.transcribe(task.audio, _transcription_options(task, self.options))
                },
                _ => {
                    //## Reject providers without transcription support
                    throw baml.errors.Unsupported {
                        message: "provider cannot transcribe audio",
                    }
                },
            }
        }
    }
}

class Transcribe {
    options: ai.transcription.TranscriptionOptions,

    function new(language: string? = null, prompt: string? = null) -> Transcribe throws never {
        Transcribe {
            options: ai.transcription.TranscriptionOptions { language: language, prompt: prompt },
        }
    }

    implements ai.Runner<ai.transcription.TranscriptionTask> {
        type Output = string
        type Error = ai.Failure | baml.errors.UnknownError

        function run(
            self,
            task: ai.transcription.TranscriptionTask,
        ) -> string throws ai.Failure | baml.errors.UnknownError {
            //# Select bounded-audio transcription and project only the text
            match (task.provider) {
                let capability: ai.transcription.TranscriptionProvider => {
                    //## Transcribe the finite audio stream
                    capability
                        .transcribe(task.audio, _transcription_options(task, self.options))
                        .value
                },
                _ => {
                    //## Reject providers without transcription support
                    throw baml.errors.Unsupported {
                        message: "provider cannot transcribe audio",
                    }
                },
            }
        }
    }
}
