class ModelStep<T> {
  outcome: T | ToolCalls,
  meta: ResponseMetadata,
}

interface ToolCallingProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Conversation
    throws root.ai.ToolError | baml.errors.UnknownError
  function step<T>(self, conversation: Conversation, tools: Tool[]) -> ModelStep<T>
    throws root.ai.ToolError | baml.errors.UnknownError
  function submit(self, conversation: Conversation, results: ToolResult[]) -> Conversation
    throws root.ai.ToolError | baml.errors.UnknownError
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_conversation(self, conversation: Conversation) -> ConversationToken
    throws baml.errors.UnknownError
  function restore_conversation(self, token: ConversationToken) -> Conversation
    throws baml.errors.UnknownError
}

interface ConversationImportProvider requires ToolCallingProvider {
  function import_messages(self, messages: Messages) -> ConversationImport
    throws baml.errors.UnknownError
}
