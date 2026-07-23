function transcribe_with_meta(
  provider: root.ai.Provider,
  stream: root.ai.AudioStream,
  options: root.ai.TranscriptionOptions,
) -> root.ai.Response<string> throws root.ai.CallError | baml.errors.UnknownError {
  match (provider) {
    let capability: root.ai.TranscriptionProvider => capability.transcribe(stream, options),
    _ => throw baml.errors.Unsupported { message: "provider cannot transcribe audio" },
  }
}

function transcribe(
  provider: root.ai.Provider,
  stream: root.ai.AudioStream,
  options: root.ai.TranscriptionOptions,
) -> string throws root.ai.CallError | baml.errors.UnknownError {
  transcribe_with_meta(provider, stream, options).value
}
