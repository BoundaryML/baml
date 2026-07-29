// Provider-owned continuation state. Concrete implementations retain wire
// identifiers, encrypted reasoning blocks, cache handles, or any other state
// needed for exact resumption while exposing a portable Messages projection.
interface Conversation {
  function provider(self) -> Provider throws never
  function messages(self) -> Messages throws never
  // Concrete providers record a versioned nominal + structural fingerprint
  // for the Task<T> they were created for. Legacy/application conversations
  // may return null.
  function output_type_fingerprint(self) -> string? throws never { null }

  // Continue this exact provider-owned state with fresh application messages.
  // The append is local; the next Agent.run performs the model request.
  function append_messages(
    self,
    messages: Messages,
  ) -> Conversation
      throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    match (self.provider()) {
      let capability: ConversationAppendProvider => {
        capability.append_messages(self, messages)
      },
      let provider: Provider => throw baml.errors.Unsupported {
        message: "provider cannot append exact conversation messages: " + provider.name(),
      },
    }
  }

  function append_message(
    self,
    message: Message,
  ) -> Conversation
      throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    self.append_messages(MessageHistory { entries: [message] })
  }
}

// A provider-controlled in-process conversation. `sealed_state` stands in for
// wire IDs, thinking signatures, encrypted blocks, and other exact state.
class ProviderConversation {
    owner: Provider,
    history: MessageHistory,
    sealed_state: json,
    declared_output_type: string?,

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function messages(self) -> Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

class ConversationToken {
    provider: string,
    version: int,
    sealed: string,
}

enum ConversationFidelity {
    Exact,
    MessagesOnly,
    Lossy,
}

class ConversationImport {
    conversation: Conversation,
    fidelity: ConversationFidelity,
    warnings: string[],
}
