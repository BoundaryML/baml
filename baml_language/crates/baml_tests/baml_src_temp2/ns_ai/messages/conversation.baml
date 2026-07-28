// Provider-owned continuation state. Concrete implementations retain wire
// identifiers, encrypted reasoning blocks, cache handles, or any other state
// needed for exact resumption while exposing a portable Messages projection.
interface Conversation {
  function provider(self) -> Provider throws never
  function messages(self) -> Messages throws never
}

// A provider-controlled in-process conversation. `sealed_state` stands in for
// wire IDs, thinking signatures, encrypted blocks, and other exact state.
class ProviderConversation {
    owner: Provider,
    history: MessageHistory,
    sealed_state: json,

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function messages(self) -> Messages throws never {
            self.history
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
