function InspectImage(value: image) -> string {
    provider: ai.testing.fake_output_provider(`"fixture image"`)
    prompt: `Describe this image in one short sentence: ${value}`
}

function InspectPdf(value: pdf) -> string {
    provider: ai.testing.fake_output_provider(`"fixture PDF"`)
    prompt: `Summarize this PDF: ${value}`
}

// VoiceSupport is intentionally not directly drivable when the selected
// provider is realtime-only.
function VoiceSupport(instructions: string) -> null {
    provider: ai.testing.fake_output_provider("null")
    prompt: `${instructions}`
}

// The ordinary direct-call lowering exists, but a realtime-only provider will
// fail capability checking; callers explicitly open its LiveSession resource.
// Future compiler lowering:
//   TranscribeCall.task(audio, $provider = provider)
// becomes this hand-written function-specific task companion. The task owns
// both the provider and its strongly typed audio argument.
function TranscribeCall_task(
    audio: ai.transcription.AudioStream,
    provider: ai.Provider,
) -> ai.transcription.TranscriptionTask throws never {
    ai.transcription.TranscriptionTask {
        provider: provider,
        audio: audio,
        prompt: "Transcribe this finite customer-support recording.",
        identity: ai.TaskIdentity { name: "TranscribeCall", package: "user" },
    }
}
