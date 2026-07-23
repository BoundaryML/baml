class MessagePart {
  text: string?,
  image: image?,
  audio: audio?,
  pdf: pdf?,
  video: video?,

  function from_text(text: string) -> MessagePart throws never {
    MessagePart { text: text }
  }
  function from_image(value: image) -> MessagePart throws never {
    MessagePart { image: value }
  }
  function from_audio(value: audio) -> MessagePart throws never {
    MessagePart { audio: value }
  }
  function from_pdf(value: pdf) -> MessagePart throws never {
    MessagePart { pdf: value }
  }
  function from_video(value: video) -> MessagePart throws never {
    MessagePart { video: value }
  }
}

class ChatMessage {
  role: string,
  parts: MessagePart[],

  function user(text: string) -> ChatMessage throws never {
    ChatMessage { role: "user", parts: [MessagePart.from_text(text)] }
  }
  function system(text: string) -> ChatMessage throws never {
    ChatMessage { role: "system", parts: [MessagePart.from_text(text)] }
  }
  function assistant(text: string) -> ChatMessage throws never {
    ChatMessage { role: "assistant", parts: [MessagePart.from_text(text)] }
  }
  function text(self) -> string throws never {
    self.parts.map((part: MessagePart) -> string { part.text ?? "" }).join("")
  }
}

function render_messages(messages: ChatMessage[]) -> string throws never {
  messages.map((message: ChatMessage) -> string {
    message.role + ": " + message.text()
  }).join("\n")
}

function prompt_to_messages(rendered_prompt: baml.llm.PromptAst) -> ChatMessage[] throws never {
  [ChatMessage.user(baml.llm.prompt_to_text(rendered_prompt))]
}
