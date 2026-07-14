interface Tools requires Provider {
  // Keep the provider-owned transcript representation hidden when `Tools` is
  // used as an existential; concrete adapters bind their exact wire state.
  type Transcript = unknown
  function begin<T>(self, request: Request<T>) -> Self.Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Self.Transcript) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError

  function submit(self, transcript: Self.Transcript, results: ToolResult[]) -> Self.Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  // Providers that support changing the concrete roster override this. The
  // identity default preserves compatibility for fixed-roster adapters.
  function sync_tools(self, transcript: Self.Transcript, tools: Tool[]) -> Self.Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError {
    transcript
  }
  function supports_dynamic_tools(self) -> bool throws never { false }
}

// Optional telemetry side-channel. It is deliberately non-generic so wrapping
// `T | ToolCalls` does not change the output type seen by SAP in existing
// providers. Adapters update this after each step and may preserve reasoning,
// provider metadata, and usage here.
interface ToolTelemetry requires Tools {
  function last_step_meta(self) -> ToolStepMeta throws never
}
