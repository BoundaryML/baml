implements root.ai.tools.ResumableToolCallingProvider for Gemini {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Prompt tool conversations do not have serializable provider-native state",
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
        "Prompt tool conversations do not have serializable provider-native state",
      );
    }
    root.google.vertex.internal._restore(self, token)
  }
}

implements root.ai.tools.ConversationImportProvider for Gemini {
  function import_messages(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    if (self.tool_mode == ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Prompt tool mode cannot import a conversation without the original Task render recipe",
      );
    }
    root.google.vertex.internal._import(self, messages)
  }
}
