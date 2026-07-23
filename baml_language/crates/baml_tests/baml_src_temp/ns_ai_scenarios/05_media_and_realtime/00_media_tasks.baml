// Future compiler lowering:
//   InspectImage.task(image, $provider = provider)
// becomes this hand-written InspectImage_task companion.
function InspectImage_task(
  value: image,
  provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
  root.ai.task_named<string>(
    provider,
    "InspectImage",
    { "image": value },
    [],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["Describe this image in one short sentence: ", ""],
        [value],
      )
    },
  )
}

function InspectImage_manual(value: image, provider: root.ai.Provider) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<string>(InspectImage_task(value, provider))
}

// Future compiler lowering for a PDF-valued domain argument.
function InspectPdf_task(value: pdf, provider: root.ai.Provider) -> root.ai.Task<string> throws never {
  root.ai.task_named<string>(
    provider,
    "InspectPdf",
    { "pdf": value },
    [],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(["Summarize this PDF: ", ""], [value])
    },
  )
}

function InspectPdf_manual(value: pdf, provider: root.ai.Provider) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<string>(InspectPdf_task(value, provider))
}

// Future compiler lowering:
//   VoiceSupport.task(instructions, $provider = provider)
// becomes this hand-written VoiceSupport_task companion. It is intentionally
// not directly drivable when the provider is realtime-only.
function VoiceSupport_task(
  instructions: string,
  provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
  root.ai.task_named<string>(
    provider,
    "VoiceSupport",
    { "instructions": instructions },
    [],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(["", ""], [instructions])
    },
  )
}

// The ordinary direct-call lowering exists, but a realtime-only provider will
// fail capability checking; callers use root.ai.drivers.open_live instead.
function VoiceSupport_manual(instructions: string, provider: root.ai.Provider) -> string
    throws baml.errors.CallError | baml.errors.UnknownError {
  root.ai.drivers.drive<string>(VoiceSupport_task(instructions, provider))
}
