// Editable, provider-neutral conversation data for UI, export, compaction, and
// cross-provider handoff. It is never sufficient by itself to resume a harness.
class ConversationMessage {
  role: string,
  parts: MessagePart[],
  provider_metadata: json?,
}

class Conversation {
  messages: ConversationMessage[],
}

function conversation_from_messages(messages: ChatMessage[]) -> Conversation throws never {
  Conversation {
    messages: messages.map((message: ChatMessage) -> ConversationMessage {
      ConversationMessage {
        role: message.role,
        parts: message.parts,
        provider_metadata: null,
      }
    }),
  }
}

// Provider-controlled exact continuation state. Applications may persist and
// transport this value, but must not decode, edit, compact, or synthesize it.
class HarnessSessionToken {
  provider: string,
  runtime: string,
  session_id: string,
  version: int,
  sealed: string,
  expires_at: string?,
}
