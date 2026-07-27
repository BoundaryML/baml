class Messages {
    model: string,
    api_key: string?,
    base_url: string?,
    extra_headers: map<string, string>?,
    extra_body: map<string, unknown>?,
}

implements root.ai.Provider for Messages {
  function name(self) -> string throws never { "anthropic" }
  function render_shorthand(self) -> string throws never { "anthropic/" + self.model }
}

implements root.ai.GenerationProvider for Messages {
  function generate<T>(self, task: root.ai.Task<T>) -> root.ai.Response<T>
      throws baml.errors.UnknownError {
    primitive_generate<T>(
      configured_primitive(
        self.render_shorthand(),
        self.model,
        self.api_key,
        self.base_url,
        self.extra_headers,
        self.extra_body,
      ),
      task,
      self.model,
    )
  }
}

implements root.ai.CompletionProvider for Messages {
  function complete<T>(self, task: root.ai.Task<T>) -> root.ai.Response<T>
      throws baml.errors.UnknownError {
    self.generate<T>(task)
  }

  function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
    root.ai.ReplayPolicy {
      kind: root.ai.ReplayKind.Safe,
      idempotency_key: null,
    }
  }
}

implements root.ai.StreamingProvider for Messages {
  function stream<TPartial, T>(
    self,
    task: root.ai.Task<T>,
  ) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
    primitive_stream<TPartial, T>(
      configured_primitive(
        self.render_shorthand(),
        self.model,
        self.api_key,
        self.base_url,
        self.extra_headers,
        self.extra_body,
      ),
      task,
    )
  }
}

function messages(
    model: string = "claude-sonnet-4-6",
    api_key: string? = null,
    base_url: string? = null,
    extra_headers: map<string, string>? = null,
    extra_body: map<string, unknown>? = null,
) -> Messages throws never {
    Messages {
        model: model,
        api_key: api_key,
        base_url: base_url,
        extra_headers: extra_headers,
        extra_body: extra_body,
    }
}
