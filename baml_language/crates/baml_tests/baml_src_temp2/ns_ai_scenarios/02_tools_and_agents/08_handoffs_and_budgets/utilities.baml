class LiveCostedToolProvider {
    inner: root.openai.Chat,
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

implements root.ai.ToolCallingProvider for LiveCostedToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
      throws root.ai.ToolError | baml.errors.UnknownError {
    self.inner.begin<T>(task.with_provider(self.inner))
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.Tool[])
      -> root.ai.ModelStep<T> throws root.ai.ToolError | baml.errors.UnknownError {
    let result = self.inner.step<T>(conversation, tools);
    match (result.meta.usage) {
      let usage: root.ai.Usage => { usage.cost_usd = self.cost_per_step_usd; },
      null => {},
    }
    result
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.ToolResult[],
  ) -> root.ai.Conversation throws root.ai.ToolError | baml.errors.UnknownError {
    self.inner.submit(conversation, results)
  }
}
