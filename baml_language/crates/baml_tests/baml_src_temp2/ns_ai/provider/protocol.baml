// Provider configuration and prompt-rendering identity. Normal model execution
// is defined by AgentProvider below; other capabilities such as realtime and
// background jobs remain separate.
interface Provider {
  function name(self) -> string throws never { "provider" }
  // The provider owns the primitive client used to render its prompt. There is
  // no portable default because role, media, and schema behavior vary by
  // provider.
  function render_shorthand(self) -> string throws never
}

// The result of exactly one provider turn. Application ToolCalls ask the
// Agent runner to execute functions; T finishes the task.
class ModelStep<T> {
    outcome: T | root.ai.tools.ToolCalls,
    metadata: ResponseMetadata,
}

// The normal model protocol. Providers own prompt/wire conversion and exact
// continuation state. They never execute application tool handlers and never
// own the loop: ai.run.Agent does.
//
// begin creates local provider state without making a model request.
// step performs exactly one model request.
// submit validates and records one correlated result per pending tool call
// without asking the model for another turn.
//
// Transactional failure invariant: when step throws, it MUST leave the
// supplied Conversation unchanged. Retry may call step again with that exact
// object when the failure is classified as replay-safe. A provider must only
// commit local continuation mutations once it can return a ModelStep.
interface AgentProvider requires Provider {
  function begin<T>(self, task: Task<T>) -> Conversation
    throws root.ai.Failure | baml.errors.UnknownError
  function step<T>(self, conversation: Conversation, tools: root.ai.tools.Tool[]) -> ModelStep<T>
    throws root.ai.Failure | baml.errors.UnknownError
  function submit(
    self,
    conversation: Conversation,
    results: root.ai.tools.ToolResult[],
  ) -> Conversation
    throws root.ai.Failure | baml.errors.UnknownError
}

interface ResumableAgentProvider requires AgentProvider {
  function save_conversation(self, conversation: Conversation) -> ConversationToken
    throws baml.errors.UnknownError
  function restore_conversation(self, token: ConversationToken) -> Conversation
    throws baml.errors.UnknownError
}

interface ConversationImportProvider requires AgentProvider {
  function import_messages<T>(self, messages: Messages) -> ConversationImport
    throws baml.errors.UnknownError
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported
}
