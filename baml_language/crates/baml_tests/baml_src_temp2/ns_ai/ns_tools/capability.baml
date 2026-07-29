class ModelStep<T> {
  outcome: T | ToolCalls,
  metadata: root.ai.ResponseMetadata,
}

interface ToolCallingProvider requires root.ai.Provider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation
    throws root.ai.Failure | baml.errors.UnknownError
  function step<T>(self, conversation: root.ai.Conversation, tools: Tool[]) -> ModelStep<T>
    throws root.ai.Failure | baml.errors.UnknownError
  function submit(self, conversation: root.ai.Conversation, results: ToolResult[]) -> root.ai.Conversation
    throws root.ai.Failure | baml.errors.UnknownError
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_conversation(self, conversation: root.ai.Conversation) -> root.ai.ConversationToken
    throws baml.errors.UnknownError
  function restore_conversation(self, token: root.ai.ConversationToken) -> root.ai.Conversation
    throws baml.errors.UnknownError
}

interface ConversationImportProvider requires ToolCallingProvider {
  function import_messages(self, messages: root.ai.Messages) -> root.ai.ConversationImport
    throws baml.errors.UnknownError
}
