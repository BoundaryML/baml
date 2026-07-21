class Anthropic {
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
}

implements Provider for Anthropic {
  function name(self) -> string throws never { "anthropic" }
  function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
}

implements GenerationProvider for Anthropic {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    primitive_generate<T>(
      configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      task,
      "anthropic",
      self.model,
    )
  }
}

implements DriveProvider for Anthropic {
  function drive<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.generate<T>(task)
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
  }
}

implements StreamingProvider for Anthropic {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
      throws baml.errors.UnknownError {
    primitive_stream<TPartial, T>(
      configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      task,
      "anthropic",
    )
  }
}

function anthropic() -> Anthropic throws never {
  Anthropic {
    model: "claude-sonnet-4-6",
    api_key: null,
    base_url: null,
    extra_headers: null,
    extra_body: null,
  }
}
