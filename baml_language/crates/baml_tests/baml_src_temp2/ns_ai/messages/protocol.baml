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

interface Message {
  function role(self) -> MessageRole throws never
  function parts(self) -> MessagePart[] throws never
  function provider_metadata(self) -> json throws never { null }
}

interface Messages {
  function items(self) -> Message[] throws never
  function append(self, message: Message) -> Messages throws never
  function to_history(self) -> MessageHistory throws never
}
