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

class ConversationProbe {
  branch: "billing" | "fraud" | "general",
  evidence: string,
}

function conversation_text(conversation: root.ai.Conversation) -> string throws never {
  conversation.entries.map((message: root.ai.Message) -> string {
    message.role().to_string() + ": " + root.ai.message_text(message)
  }).join("\n")
}

function ConversationProbe_task(
  conversation: root.ai.Conversation,
  provider: root.ai.Provider,
) -> root.ai.Task<ConversationProbe> throws never {
  root.ai.task_named<ConversationProbe>(
    provider,
    "ConversationProbe",
    { "conversation": conversation },
    [],
    default_task_options(),
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Read this application-owned conversation. Classify the final user request as billing when it asks for a refund, fraud when it asks about fraud, or general otherwise. Copy the most relevant concrete fact into evidence.\n${conversation_text(conversation)}`,
          ctx.output_format,
        ],
      )
    },
  )
}
