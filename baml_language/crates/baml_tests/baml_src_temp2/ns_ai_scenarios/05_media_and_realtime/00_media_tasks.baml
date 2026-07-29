// Future compiler lowering:
//   InspectImage.task(image, $provider = provider)
// becomes this hand-written InspectImage_task companion.
function InspectImage_task(
    value: image,
    provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
    root.ai.task_named<string>(provider, "InspectImage", { "image": value }, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Describe this image in one short sentence: ", ""], [value])
    })
}

function InspectImage_manual(
    value: image,
    provider: root.ai.Provider,
) -> string throws root.ai.Failure | baml.errors.UnknownError {
    root.ai
        .internal
        ._require_done(
            InspectImage_task(value, provider).run(runner = root.ai.run.Agent<string>.new()),
        )
        .value
}

// Future compiler lowering for a PDF-valued domain argument.
function InspectPdf_task(
    value: pdf,
    provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
    root.ai.task_named<string>(provider, "InspectPdf", { "pdf": value }, [], (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Summarize this PDF: ", ""], [value])
    })
}

function InspectPdf_manual(
    value: pdf,
    provider: root.ai.Provider,
) -> string throws root.ai.Failure | baml.errors.UnknownError {
    root.ai
        .internal
        ._require_done(
            InspectPdf_task(value, provider).run(runner = root.ai.run.Agent<string>.new()),
        )
        .value
}

// Future compiler lowering:
//   VoiceSupport.task(instructions, $provider = provider)
// becomes this hand-written VoiceSupport_task companion. It is intentionally
// not directly drivable when the provider is realtime-only.
function VoiceSupport_task(
    instructions: string,
    provider: root.ai.Provider,
) -> root.ai.Task<null> throws never {
    root.ai.task_named<null>(provider, "VoiceSupport", { "instructions": instructions }, [], (
        ctx: baml.llm.Context,
    ) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["", ""], [instructions])
    })
}

// The ordinary direct-call lowering exists, but a realtime-only provider will
// fail capability checking; callers explicitly open its LiveSession resource.
function VoiceSupport_manual(
    instructions: string,
    provider: root.ai.Provider,
) -> null throws root.ai.Failure | baml.errors.UnknownError {
    root.ai
        .internal
        ._require_done(
            VoiceSupport_task(instructions, provider).run(runner = root.ai.run.Agent<null>.new()),
        )
        .value
}

// Future compiler lowering:
//   TranscribeCall.task(audio, $provider = provider)
// becomes this hand-written function-specific task companion. The task owns
// both the provider and its strongly typed audio argument.
function TranscribeCall_task(
    audio: root.ai.transcription.AudioStream,
    provider: root.ai.Provider,
) -> root.ai.transcription.TranscriptionTask throws never {
    root.ai.transcription.TranscriptionTask {
        provider: provider,
        audio: audio,
        prompt: "Transcribe this finite customer-support recording.",
        identity: root.ai.TaskIdentity { name: "TranscribeCall", package: "bepv2_reference" },
    }
}
