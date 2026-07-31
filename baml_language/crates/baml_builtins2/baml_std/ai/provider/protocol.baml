// Provider configuration and prompt-rendering identity. Normal model execution
// is defined by AgentProvider below; other capabilities such as realtime and
// background jobs remain separate.
interface Provider {
  function name(self) -> string throws never { "provider" }

  /// The prompt-rendering identity, in `vendor/model` form (e.g.
  /// "openai/gpt-5.6-luna", "acme/acme-1"). Any two non-empty segments
  /// joined by "/" are valid; the value selects prompt-rendering defaults
  /// and never needs to be a real vendor. A value that does not parse
  /// raises `ai.InvalidRequest` at first render.
  function render_shorthand(self) -> string throws never

  /// The provider this one forwards to, when it is a thin wrapper (retry,
  /// middleware, instrumentation). Conversation-ownership checks walk this
  /// chain, so a delegating wrapper is legal at any nesting depth. Leaf
  /// providers keep the default null.
  function delegate(self) -> Provider? throws never { null }

  // Prompt rendering always resolves through a primitive client. Most
  // providers derive one from their shorthand; the compiler's client adapter
  // overrides this to retain exact named-client role and media settings.
  function _render_client(self) -> baml.llm.PrimitiveClient throws ai.InvalidRequest {
    baml.llm.from_shorthand(self.render_shorthand()) catch (e) {
      _ => throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "invalid provider render shorthand \""
            + self.render_shorthand()
            + "\": expected \"vendor/model\"",
      }
    }
  }
}

/// True when the two values are the SAME provider instance — not merely
/// equal configurations — or when one reaches the other through its
/// `delegate()` chain. The public form of the ownership identity every
/// conversation check uses; custom providers and wrappers use it to reject
/// another instance's conversation with the same rule the stdlib applies.
function same_provider_instance(left: Provider, right: Provider) -> bool throws never {
    ai.internal._same_provider_instance(left, right)
}

// The result of exactly one provider turn. Application ToolCalls ask the
// Agent runner to execute functions; T finishes the task.
class ModelStep<T> {
    outcome: T | ai.tools.ToolCalls,
    metadata: ResponseMetadata,
    // Provider-neutral display channels, populated when the vendor exposes
    // them: assistant preamble/visible text alongside the outcome, and
    // reasoning text (Anthropic thinking text, OpenAI reasoning summaries,
    // Gemini thought summaries). Signed/encrypted continuation blobs stay
    // in the Conversation; these are the application-facing projection.
    assistant_text: string?,
    reasoning_text: string?,
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
/// The atomicity rule every implementation MUST obey: `step`, `submit`,
/// and (via ConversationAppendProvider) `append_messages` mutate their
/// conversation state only AFTER wire success. A failing call leaves the
/// conversation exactly as it was. This is what makes step-level retry
/// replay-safe and what lets the Agent loop guarantee that a throw means
/// "nothing changed" while a `Failed` outcome means "here is the committed
/// state" (D11).
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
// Appending MUTATES the supplied conversation in place and is statement-
// shaped (returns null); a failing append MUST leave the conversation
// unchanged (the adapter atomicity rule). Appending is local: it MUST NOT
// make a model request. Providers reject conversations owned by another
// provider instance and conversations with unresolved tool calls.
interface ConversationAppendProvider requires AgentProvider {
  function append_messages(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> null
    throws ai.Failure | baml.errors.UnknownError

  function append_message(
    self,
    conversation: Conversation,
    message: Message,
  ) -> null
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
