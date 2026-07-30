// Provider configuration and prompt-rendering identity. Normal model execution
// is defined by AgentProvider below; other capabilities such as realtime and
// background jobs remain separate.
interface Provider {
  function name(self) -> string throws never { "provider" }
  function render_shorthand(self) -> string throws never

  // Prompt rendering always resolves through a primitive client. Most
  // providers derive one from their shorthand; the compiler's client adapter
  // overrides this to retain exact named-client role and media settings.
  function _render_client(self) -> baml.llm.PrimitiveClient throws never {
    baml.llm.from_shorthand(self.render_shorthand()) catch (e) {
      _ => baml.sys.panic("ai: invalid provider render shorthand")
    }
  }
}

// The result of exactly one provider turn. Application ToolCalls ask the
// Agent runner to execute functions; T finishes the task.
class ModelStep<T> {
    outcome: T | ai.tools.ToolCalls,
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
    throws ai.Failure | baml.errors.UnknownError
  function step<T>(self, conversation: Conversation, tools: ai.tools.Tool[]) -> ModelStep<T>
    throws ai.Failure | baml.errors.UnknownError
  function submit(
    self,
    conversation: Conversation,
    results: ai.tools.ToolResult[],
  ) -> Conversation
    throws ai.Failure | baml.errors.UnknownError
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

// Adds a fresh application message to an exact provider continuation without
// reconstructing it from the portable Messages projection. Implementations
// retain opaque response IDs, thinking signatures, cache handles, and the
// Conversation's original output-type fingerprint.
//
// Appending is local: it MUST NOT make a model request. Providers reject
// conversations owned by another provider instance and conversations with
// unresolved tool calls.
interface ConversationAppendProvider requires AgentProvider {
  function append_messages(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> Conversation
    throws ai.Failure | baml.errors.UnknownError

  function append_message(
    self,
    conversation: Conversation,
    message: Message,
  ) -> Conversation
      throws ai.Failure | baml.errors.UnknownError {
    self.append_messages(
      conversation,
      MessageHistory { entries: [message] },
    )
  }
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported
}
