function sample_history() -> root.ai.MessageHistory throws never {
  root.ai.MessageHistory {
    entries: [
      root.ai.ChatMessage.user("I was charged twice."),
      root.ai.ChatMessage.assistant("I can help with that."),
    ],
  }
}

function compact_history(history: root.ai.MessageHistory) -> root.ai.MessageHistory throws never {
  root.ai.MessageHistory {
    entries: [root.ai.ChatMessage.user(
      "Summary: customer reports a duplicate charge; support acknowledged it.",
    )],
  }
}

class ConversationProbe {
  branch: "billing" | "fraud" | "general",
  evidence: string,
}

function history_text(history: root.ai.MessageHistory) -> string throws never {
  history.entries.map((message: root.ai.Message) -> string {
    message.role().to_string() + ": " + root.ai.message_text(message)
  }).join("\n")
}

function ConversationProbe_task(
  history: root.ai.MessageHistory,
  provider: root.ai.Provider,
) -> root.ai.Task<ConversationProbe> throws never {
  root.ai.task_named<ConversationProbe>(
    provider,
    "ConversationProbe",
    { "history": history },
    [],
    (ctx: baml.llm.Context) -> baml.llm.PromptAst {
      baml.llm.assemble_prompt_ast(
        ["", "\n", ""],
        [
          `Read this application-owned message history. Classify the final user request as billing when it asks for a refund, fraud when it asks about fraud, or general otherwise. Copy the final user message verbatim into evidence.\n${history_text(history)}`,
          ctx.output_format,
        ],
      )
    },
  )
}
