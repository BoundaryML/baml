class LiveCostedToolProvider {
    inner: root.openai.Responses,
    cost_per_step_usd: float,

    implements root.ai.Provider {
        function name(self) -> string throws never {
            "costed-" + self.inner.name()
        }
        function render_shorthand(self) -> string throws never {
            self.inner.render_shorthand()
        }
    }
}

class LiveCostedConversation {
  owner: LiveCostedToolProvider,
  inner: root.ai.Conversation,

  implements root.ai.Conversation {
    function provider(self) -> root.ai.Provider throws never { self.owner }
    function messages(self) -> root.ai.Messages throws never { self.inner.messages() }
    function output_type_fingerprint(self) -> string? throws never {
      self.inner.output_type_fingerprint()
    }
  }
}

implements root.ai.AgentProvider for LiveCostedToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    LiveCostedConversation {
      owner: self,
      inner: self.inner.begin<T>(task.with_provider(self.inner)),
    }
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.tools.Tool[])
      -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: LiveCostedConversation => value,
      _ => throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider's conversation",
      },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider instance's conversation",
      }
    }
    let result = self.inner.step<T>(state.inner, tools);
    match (result.metadata.usage) {
      let usage: root.ai.Usage => { usage.cost_usd = self.cost_per_step_usd; },
      null => {},
    }
    result
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation throws root.ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: LiveCostedConversation => value,
      _ => throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider's conversation",
      },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider instance's conversation",
      }
    }
    state.inner = self
      .inner
      .as<root.ai.AgentProvider>
      .submit(state.inner, results);
    state
  }
}
