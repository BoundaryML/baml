// Provider-neutral text projection used by compaction, display, and adapters.
function message_text(message: Message) -> string throws never {
    message
        .parts()
        .map((part: MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

class ChatMessage {
    message_role: MessageRole,
    content: MessagePart[],
    metadata: json,

    implements Message {
        function role(self) -> MessageRole throws never {
            self.message_role
        }
        function parts(self) -> MessagePart[] throws never {
            self.content
        }
        function provider_metadata(self) -> json throws never {
            self.metadata
        }
    }

    function text(self) -> string throws never {
        self.content
            .map((part: MessagePart) -> string {
                part.text() ?? ""
            })
            .join("")
    }

    function user(text: string) -> ChatMessage throws never {
        ChatMessage {
            message_role: MessageRole.User,
            content: [TextPart { value: text, metadata: {} }],
            metadata: null,
        }
    }

    function assistant(text: string) -> ChatMessage throws never {
        ChatMessage {
            message_role: MessageRole.Assistant,
            content: [TextPart { value: text, metadata: {} }],
            metadata: null,
        }
    }
}

// Editable, serializable application state. Exact provider continuation data
// remains on Conversation rather than being reconstructed from this class.
class MessageHistory {
    entries: Message[],

    implements Messages {
        function items(self) -> Message[] throws never {
            self.entries
        }
        function append(self, message: Message) -> Messages throws never {
            let entries = self.entries.slice(0, self.entries.length());
            entries.push(message);
            MessageHistory { entries: entries }
        }
        function to_history(self) -> MessageHistory throws never {
            self
        }
    }

    function empty() -> MessageHistory throws never {
        MessageHistory { entries: [] }
    }
}
