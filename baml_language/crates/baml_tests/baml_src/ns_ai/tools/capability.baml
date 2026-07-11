interface Tools requires Provider {
  type Transcript
  function begin<T>(self, request: Request<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Transcript) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError

  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}

// Optional telemetry side-channel. It is deliberately non-generic so wrapping
// `T | ToolCalls` does not change the output type seen by SAP in existing
// providers. Adapters update this after each step and may preserve reasoning,
// provider metadata, and usage here.
interface ToolTelemetry requires Tools {
  function last_step_meta(self) -> ToolStepMeta throws never
}
