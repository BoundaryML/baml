class MessagePart {
  text: string?,
  image: image?,
  audio: audio?,
  pdf: pdf?,
  video: video?,

  function from_text(text: string) -> MessagePart throws never {
    MessagePart { text: text }
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
}

function prompt_to_messages(rendered_prompt: baml.llm.PromptAst) -> ChatMessage[] throws never {
  [ChatMessage.user(baml.llm.prompt_to_text(rendered_prompt))]
}
