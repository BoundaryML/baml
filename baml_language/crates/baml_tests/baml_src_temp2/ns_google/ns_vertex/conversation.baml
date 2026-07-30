implements ai.ResumableAgentProvider for Gemini {
  function save_conversation(
    self,
    conversation: ai.Conversation,
  ) -> ai.ConversationToken throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot save a ToolMode.Prompt conversation because resuming it requires the original Task and its prompt-rendering function. Use ToolMode.Native for save and restore.",
      );
    }
    root.google.vertex.internal._save(self, conversation)
  }

  function restore_conversation(
    self,
    token: ai.ConversationToken,
  ) -> ai.Conversation throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot restore a conversation with ToolMode.Prompt because the token does not contain the original Task or its prompt-rendering function. Use ToolMode.Native for save and restore.",
      );
    }
    root.google.vertex.internal._restore(self, token)
  }
}

implements ai.ConversationImportProvider for Gemini {
  function import_messages<T>(
    self,
    messages: ai.Messages,
  ) -> ai.ConversationImport throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot import messages with ToolMode.Prompt because prompt tools require the original Task and its prompt-rendering function. Use ToolMode.Native, or start a new Task from the messages.",
      );
    }
    root.google.vertex.internal._import<T>(self, messages)
  }
}

implements ai.ConversationAppendProvider for Gemini {
  function append_messages(
    self,
    conversation: ai.Conversation,
    messages: ai.Messages,
  ) -> ai.Conversation throws ai.Failure {
    root.google.internal._append_messages(
      self,
      conversation,
      messages,
      self.name(),
    )
  }
}
