implements root.ai.AgentProvider for Gemini {
  function begin<T>(
    self,
    task: root.ai.Task<T>,
  ) -> root.ai.Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
    root.google.vertex.internal._validate(self);
    if (self.tool_mode == ToolMode.Prompt) {
      return root.google.internal._new_prompt_conversation(
        self,
        root.google.vertex.internal._primitive(self),
        self.name(),
        self.model,
        task,
      );
    }
    root.google.vertex.internal._begin<T>(self, task)
  }

  function step<T>(
    self,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
  ) -> root.ai.ModelStep<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    root.google.vertex.internal._validate(self);
    root.ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    if (!root.ai.internal._same_provider_instance(conversation.provider(), self)) {
      throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "Google Vertex received a conversation owned by another provider instance",
      };
    }
    root.google.internal._validate_application_tool_names(tools, self.name());
    if (self.tool_mode == ToolMode.Prompt) {
      return root.google.internal._prompt_step<T>(
        conversation,
        tools,
        self.name(),
      );
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
    if (!root.ai.internal._same_provider_instance(conversation.provider(), self)) {
      throw root.ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "Google Vertex received a conversation owned by another provider instance",
      };
    }
    if (self.tool_mode == ToolMode.Prompt) {
      return root.google.internal._prompt_submit(
        conversation,
        results,
        self.name(),
      );
    }
    root.google.internal._submit(conversation, results, self.name())
  }
}
