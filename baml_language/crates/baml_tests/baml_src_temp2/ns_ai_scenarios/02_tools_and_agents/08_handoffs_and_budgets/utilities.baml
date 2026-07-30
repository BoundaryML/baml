class LiveCostedToolProvider {
    inner: root.openai.OpenAIProvider,
    cost_per_step_usd: float,

    implements ai.Provider {
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
    inner: ai.Conversation,

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }
        function messages(self) -> ai.Messages throws never {
            self.inner.messages()
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.inner.output_type_fingerprint()
        }
    }
}

implements ai.AgentProvider for LiveCostedToolProvider {
  function begin<T>(self, task: ai.Task<T>) -> ai.Conversation
      throws ai.Failure | baml.errors.UnknownError {
    LiveCostedConversation {
      owner: self,
      inner: self.inner.begin<T>(task.with_provider(self.inner)),
    }
  }

  function step<T>(self, conversation: ai.Conversation, tools: ai.tools.Tool[])
      -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: LiveCostedConversation => value,
      _ => throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider's conversation",
      },
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider instance's conversation",
      }
    }
    let result = self.inner.step<T>(state.inner, tools);
    match (result.metadata.usage) {
      let usage: ai.Usage => { usage.cost_usd = self.cost_per_step_usd; },
      null => {},
    }
    result
  }

  function submit(
    self,
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
  ) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: LiveCostedConversation => value,
      _ => throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider's conversation",
      },
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "costed provider received another provider instance's conversation",
      }
    }
    state.inner = self
      .inner
      .as<ai.AgentProvider>
      .submit(state.inner, results);
    state
  }
}
