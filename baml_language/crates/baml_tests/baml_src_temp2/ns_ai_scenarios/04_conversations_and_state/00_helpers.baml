function sample_history() -> ai.MessageHistory throws never {
    ai.MessageHistory {
        entries: [
            ai.ChatMessage.user("I was charged twice."),
            ai.ChatMessage.assistant("I can help with that."),
        ],
    }
}
function compact_history(history: ai.MessageHistory) -> ai.MessageHistory throws never {
    ai.MessageHistory {
        entries: [
            ai
                .ChatMessage
                .user("Summary: customer reports a duplicate charge; support acknowledged it."),
        ],
    }
}

class ConversationProbe {
    branch: "billing" | "fraud" | "general",
    evidence: string,
}

function history_text(history: ai.MessageHistory) -> string throws never {
    history
        .entries
        .map((message: ai.Message) -> string {
            message.role().to_string() + ": " + ai.message_text(message)
        })
        .join("\n")
}

function ProbeConversation(history: ai.MessageHistory) -> ConversationProbe {
    provider: ai.testing.fake_output_provider(`{"branch":"billing","evidence":"fixture"}`)
    prompt: `
        Read this application-owned message history. Classify the final user
        request as billing when it asks for a refund, fraud when it asks about
        fraud, or general otherwise. Copy the final user message verbatim into
        evidence.
        ${history_text(history)}
        ${ctx.output_format}
    `
}
