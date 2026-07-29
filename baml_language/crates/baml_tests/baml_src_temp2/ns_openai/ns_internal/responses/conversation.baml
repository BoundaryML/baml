class Conversation {
    owner: root.openai.Responses,
    history: root.ai.MessageHistory,
    // Initial Responses input items, or function_call_output items waiting for
    // the next step.
    input: json[],
    previous_response_id: string?,
    active_tools: json[],
    last_output: json[],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
    }
}

function _message_role_name(role: root.ai.MessageRole) -> string throws never {
    match (role) {
        root.ai.MessageRole.System => "system",
        root.ai.MessageRole.User => "user",
        root.ai.MessageRole.Assistant => "assistant",
        root.ai.MessageRole.Tool => "user",
    }
}

function _message_text(message: root.ai.Message) -> string throws never {
    message
        .parts()
        .map((part: root.ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}
