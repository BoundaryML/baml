// Bounded transcription executes a typed task. The task owns the selected
// provider and finite audio argument; the runner owns only transcription
// policy.

function _transcription_options(
    task: root.ai.TranscriptionTask,
    configured: root.ai.TranscriptionOptions,
) -> root.ai.TranscriptionOptions throws never {
    //# Merge runner overrides with the task's transcription prompt
    root.ai.TranscriptionOptions {
        language: configured.language,
        prompt: configured.prompt ?? task.prompt,
    }
}

class TranscribeWithMeta {
    options: root.ai.TranscriptionOptions,

    function new(
        language: string? = null,
        prompt: string? = null,
    ) -> TranscribeWithMeta throws never {
        TranscribeWithMeta {
            options: root.ai.TranscriptionOptions { language: language, prompt: prompt },
        }
    }

    implements root.ai.Runner<root.ai.TranscriptionTask> {
        type Output = root.ai.ResponseWithMetadata<string>
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.TranscriptionTask,
        ) -> root.ai.ResponseWithMetadata<string> throws root.ai.CallError | baml.errors.UnknownError {
            //# Select bounded-audio transcription and retain provider metadata
            match (task.provider) {
                let capability: root.ai.TranscriptionProvider => {
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
    options: root.ai.TranscriptionOptions,

    function new(language: string? = null, prompt: string? = null) -> Transcribe throws never {
        Transcribe { options: root.ai.TranscriptionOptions { language: language, prompt: prompt } }
    }

    implements root.ai.Runner<root.ai.TranscriptionTask> {
        type Output = string
        type Error = root.ai.CallError | baml.errors.UnknownError

        function run(
            self,
            task: root.ai.TranscriptionTask,
        ) -> string throws root.ai.CallError | baml.errors.UnknownError {
            //# Select bounded-audio transcription and project only the text
            match (task.provider) {
                let capability: root.ai.TranscriptionProvider => {
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
