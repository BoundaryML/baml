class Done<T> {
    value: T,
    metadata: ResponseMetadata,
    conversation: Conversation,
}

class BudgetReached {
    conversation: Conversation,
    steps_taken: int,
    reason: string,
}

class Handoff {
    // The exact provider call is retained so the application can correlate a
    // ToolResult and submit it before resuming this conversation.
    call: root.ai.tools.ToolCall,
    conversation: Conversation,
    steps_taken: int,
}
