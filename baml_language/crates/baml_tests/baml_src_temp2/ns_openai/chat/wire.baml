class ToolMessage {
    content: string?,
    tool_calls: json[]?,
}

class ToolChoice {
    message: ToolMessage,
    finish_reason: string?,
}

class ToolUsage {
    prompt_tokens: int,
    completion_tokens: int,
    prompt_tokens_details: json?,
    completion_tokens_details: json?,
}

class ToolResponse {
    id: string?,
    model: string?,
    choices: ToolChoice[],
    usage: ToolUsage?,
}
