enum MessagePartKind {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
    ProviderData,
}

enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

interface MessagePart {
  function kind(self) -> MessagePartKind throws never
  function text(self) -> string? throws never { null }
  function media(self) -> image | audio | video | pdf | null throws never { null }
  function annotations(self) -> map<string, json> throws never { {} }
}

class TextPart {
    value: string,
    metadata: map<string, json>,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.Text
        }
        function text(self) -> string? throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class ImagePart {
    value: image,
    metadata: map<string, json>,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.Image
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class AudioPart {
    value: audio,
    metadata: map<string, json>,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.Audio
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class VideoPart {
    value: video,
    metadata: map<string, json>,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.Video
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class PdfPart {
    value: pdf,
    metadata: map<string, json>,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.Pdf
        }
        function media(self) -> image | audio | video | pdf | null throws never {
            self.value
        }
        function annotations(self) -> map<string, json> throws never {
            self.metadata
        }
    }
}

class ProviderDataPart {
    provider: string,
    data: json,

    implements MessagePart {
        function kind(self) -> MessagePartKind throws never {
            MessagePartKind.ProviderData
        }
        function annotations(self) -> map<string, json> throws never {
            { "provider": self.provider, "data": self.data }
        }
    }
}

interface Message {
  function role(self) -> MessageRole throws never
  function parts(self) -> MessagePart[] throws never
  function provider_metadata(self) -> json throws never { null }
}

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

interface Messages {
  function items(self) -> Message[] throws never
  function append(self, message: Message) -> Messages throws never
  function to_history(self) -> MessageHistory throws never
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

function prompt_role_or_user(role: string) -> string throws never {
    if (role.length() == 0) {
        "user"
    } else {
        role
    }
}

function prompt_text_content(prompt: baml.llm.PromptAst) -> string throws never {
    let messages = prompt.messages();
    messages
        .map((message: baml.llm.PromptMessage) -> string {
            message.content
        })
        .join("\n")
}

function prompt_message_role(role: string) -> MessageRole throws never {
    if (role == "system") {
        return MessageRole.System;
    }
    if (role == "assistant") {
        return MessageRole.Assistant;
    }
    if (role == "tool") {
        return MessageRole.Tool;
    }
    MessageRole.User
}

function prompt_to_messages(prompt: baml.llm.PromptAst) -> Messages throws never {
    MessageHistory {
        entries: prompt.messages().map((message: baml.llm.PromptMessage) -> Message {
            ChatMessage {
                message_role: prompt_message_role(message.role),
                content: [TextPart { value: message.content, metadata: {} }],
                metadata: { "prompt_role": prompt_role_or_user(message.role) },
            }
        }),
    }
}
