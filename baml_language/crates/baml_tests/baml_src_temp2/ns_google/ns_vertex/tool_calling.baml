implements root.ai.tools.ToolCallingProvider for Gemini {
  function begin<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    root.google.vertex.internal._validate(self);
    if (self.tool_mode == ToolMode.Prompt) {
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.begin<T>(task);
    }
    root.google.vertex.internal._begin<T>(self, task)
  }

  function step<T>(
    self,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
  ) -> root.ai.tools.ModelStep<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    root.google.vertex.internal._validate(self);
    if (self.tool_mode == ToolMode.Prompt) {
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.step<T>(conversation, tools);
    }
    root.google.internal._step<T>(
      conversation,
      tools,
      self.name(),
      root.google.vertex.internal._use_sap(self),
      self.parallel_tool_calls,
    )
  }

  function submit(
    self,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> root.ai.Conversation
      throws root.ai.Failure {
    root.google.vertex.internal._validate(self);
    if (self.tool_mode == ToolMode.Prompt) {
      let prompt_tools = root.ai.tools.PromptToolProvider { inner: self };
      return prompt_tools.submit(conversation, results);
    }
    root.google.internal._submit(conversation, results, self.name())
  }
}
