function _prompt_role_or_user(role: string) -> string throws never {
    if (role.length() == 0) {
        "user"
    } else {
        role
    }
}

function _prompt_text_content(prompt: baml.llm.PromptAst) -> string throws never {
    let messages = prompt.messages();
    messages
        .map((message: baml.llm.PromptMessage) -> string {
            message.content
        })
        .join("\n")
}

function _prompt_message_role(role: string) -> MessageRole throws never {
    if (role == "system") {
        return MessageRole.System;
    }
    if (role == "assistant") {
        return MessageRole.Assistant;
    }
    if (role == "tool") {
        return MessageRole.Tool;
    }
    MessageRole.User
}

function _prompt_to_messages(prompt: baml.llm.PromptAst) -> Messages throws never {
    MessageHistory {
        entries: prompt.messages().map((message: baml.llm.PromptMessage) -> Message {
            ChatMessage {
                message_role: _prompt_message_role(message.role),
                content: [TextPart { value: message.content, metadata: {} }],
                metadata: { "prompt_role": _prompt_role_or_user(message.role) },
            }
        }),
    }
}
