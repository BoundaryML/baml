interface Tools requires Provider {
  type Transcript
  function begin<T>(self, request: Request<T>) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
  function step<T>(self, transcript: Transcript) -> T | ToolCalls
    throws baml.errors.ToolError | baml.errors.UnknownError
  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript
    throws baml.errors.ToolError | baml.errors.UnknownError
}
