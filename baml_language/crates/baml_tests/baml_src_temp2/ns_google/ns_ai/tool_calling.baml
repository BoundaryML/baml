implements ai.AgentProvider for Gemini {
  function begin<T>(
    self,
    task: ai.Task<T>,
  ) -> ai.Conversation
      throws ai.Failure | baml.errors.UnknownError {
    root.google.ai.internal._validate(self);
    if (self.tool_mode == ToolMode.Prompt) {
      return root.google.internal._new_prompt_conversation(
        self,
        root.google.ai.internal._primitive(self),
        self.name(),
        self.model,
        task,
      );
    }
    root.google.ai.internal._begin<T>(self, task)
  }

  function step<T>(
    self,
    conversation: ai.Conversation,
    tools: ai.tools.Tool[],
  ) -> ai.ModelStep<T>
      throws ai.Failure | baml.errors.UnknownError {
    root.google.ai.internal._validate(self);
    ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    if (!ai.internal._same_provider_instance(conversation.provider(), self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "Google AI received a conversation owned by another provider instance",
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
      root.google.ai.internal._use_sap(self),
      self.parallel_tool_calls,
    )
  }

  function submit(
    self,
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
  ) -> ai.Conversation
      throws ai.Failure {
    root.google.ai.internal._validate(self);
    if (!ai.internal._same_provider_instance(conversation.provider(), self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "Google AI received a conversation owned by another provider instance",
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
