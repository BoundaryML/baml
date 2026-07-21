class OpenAi {
  model: string,
  api_key: string?,
  base_url: string?,
  extra_headers: map<string, string>?,
  extra_body: map<string, unknown>?,
}

implements Provider for OpenAi {
  function name(self) -> string throws never { "openai" }
  function render_shorthand(self) -> string throws never { "openai/" + self.model }
}

implements GenerationProvider for OpenAi {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    primitive_generate<T>(
      configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      task,
      "openai",
      self.model,
    )
  }
}

implements DriveProvider for OpenAi {
  function drive<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.generate<T>(task)
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
  }
}

implements StreamingProvider for OpenAi {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
      throws baml.errors.UnknownError {
    primitive_stream<TPartial, T>(
      configured_primitive(
        self.render_shorthand(), self.model, self.api_key, self.base_url,
        self.extra_headers, self.extra_body,
      ),
      task,
      "openai",
    )
  }
}

function openai() -> OpenAi throws never {
  OpenAi {
    model: "gpt-5.6-luna",
    api_key: null,
    base_url: null,
    extra_headers: null,
    extra_body: null,
  }
}
