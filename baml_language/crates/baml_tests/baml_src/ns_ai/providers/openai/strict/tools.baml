// OpenAI strict uses the same Chat Completions tool transcript as OpenAi.
// The shared implementation emits strict schemas for every function tool and
// attaches a strict response_format when the final BAML result is structured.
implements Tools for OpenAiStrict {
  type Transcript = OpenAiToolTranscript

  function begin<T>(self, request: Request<T>) -> OpenAiToolTranscript
      throws baml.errors.ToolError | baml.errors.UnknownError {
    OpenAi {
      model: self.model,
      api_key: self.api_key,
      base_url: self.base_url,
      extra_headers: self.extra_headers,
      extra_body: self.extra_body,
      append_output_schema: false,
    }.begin<T>(request)
  }

  function step<T>(self, transcript: OpenAiToolTranscript) -> T | ToolCalls
      throws baml.errors.ToolError | baml.errors.UnknownError {
    OpenAi {
      model: self.model,
      api_key: self.api_key,
      base_url: self.base_url,
      extra_headers: self.extra_headers,
      extra_body: self.extra_body,
      append_output_schema: false,
    }.step<T>(transcript)
  }

  function submit(
    self,
    transcript: OpenAiToolTranscript,
    results: ToolResult[],
  ) -> OpenAiToolTranscript throws baml.errors.ToolError | baml.errors.UnknownError {
    OpenAi {
      model: self.model,
      api_key: self.api_key,
      base_url: self.base_url,
      extra_headers: self.extra_headers,
      extra_body: self.extra_body,
      append_output_schema: false,
    }.submit(transcript, results)
  }
}
