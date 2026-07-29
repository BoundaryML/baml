implements root.ai.ResumableAgentProvider for Gemini {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
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
    token: root.ai.ConversationToken,
  ) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot restore a conversation with ToolMode.Prompt because the token does not contain the original Task or its prompt-rendering function. Use ToolMode.Native for save and restore.",
      );
    }
    root.google.vertex.internal._restore(self, token)
  }
}

implements root.ai.ConversationImportProvider for Gemini {
  function import_messages<T>(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot import messages with ToolMode.Prompt because prompt tools require the original Task and its prompt-rendering function. Use ToolMode.Native, or start a new Task from the messages.",
      );
    }
    root.google.vertex.internal._import<T>(self, messages)
  }
}
