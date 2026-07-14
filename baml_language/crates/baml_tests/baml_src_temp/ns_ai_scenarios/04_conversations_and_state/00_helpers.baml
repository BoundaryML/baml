function sample_conversation() -> root.ai.Conversation throws never {
  root.ai.Conversation {
    entries: [
      root.ai.ChatMessage.user("I was charged twice."),
      root.ai.ChatMessage.assistant("I can help with that."),
    ],
  }
}

function compact_conversation(conversation: root.ai.Conversation) -> root.ai.Conversation throws never {
  root.ai.Conversation {
    entries: [root.ai.ChatMessage.user(
      "Summary: customer reports a duplicate charge; support acknowledged it.",
    )],
  }
}

