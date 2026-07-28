class Done<T> {
    value: T,
    meta: ResponseMetadata,
    conversation: Conversation,
}

class BudgetReached {
    conversation: Conversation,
    steps_taken: int,
    reason: string,
}

class Handoff {
    to: string,
    args: json,
    conversation: Conversation,
    steps_taken: int,
}
