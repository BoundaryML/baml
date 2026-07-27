class Conversation {
    owner: Chat,
    history: root.ai.MessageHistory,
    tools_json: json[],
    wire_messages: json[],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
    }
}

function message_role_name(role: root.ai.MessageRole) -> string throws never {
    match (role) {
        root.ai.MessageRole.System => "system",
        root.ai.MessageRole.User => "user",
        root.ai.MessageRole.Assistant => "assistant",
        root.ai.MessageRole.Tool => "tool",
    }
}

function message_text(message: root.ai.Message) -> string throws never {
    message
        .parts()
        .map((part: root.ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function message_provider_data(message: root.ai.Message) -> json? throws never {
    for (let part in message.parts()) {
        match (part) {
            let data: root.ai.ProviderDataPart => {
                if (data.provider == "openai") {
                    return data.data;
                }
            },
            _ => null,
        }
    }
    null
}

implements root.ai.ConversationImportProvider for Chat {
  function import_messages(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws never {
    let history = messages.to_history();
    let wire: json[] = [];
    for (let message in history.entries) {
      let provider_data = message_provider_data(message);
      match (message.role()) {
        root.ai.MessageRole.Assistant => {
          let tool_calls = baml.json.path_or<json[]?>(provider_data, ".tool_calls", null);
          match (tool_calls) {
            let calls: json[] => {
              let frame: json = { "role": "assistant", "tool_calls": calls };
              wire.push(frame);
            },
            null => {
              let frame: json = { "role": "assistant", "content": message_text(message) };
              wire.push(frame);
            },
          };
        },
        root.ai.MessageRole.Tool => {
          let call_id = baml.json.path_or<string?>(provider_data, ".tool_call_id", null);
          match (call_id) {
            let id: string => {
              let frame: json = {
                "role": "tool",
                "tool_call_id": id,
                "content": baml.json.stringify(baml.json.path_or(provider_data, ".output", null)),
              };
              wire.push(frame);
            },
            // A foreign/provider-neutral tool result has no valid OpenAI call
            // ID. Preserve it as user-visible context instead of emitting an
            // orphan `role: tool` frame that OpenAI must reject.
            null => {
              let frame: json = {
                "role": "user",
                "content": "Imported tool result: " + baml.json.stringify(provider_data),
              };
              wire.push(frame);
            },
          };
        },
        _ => {
          let frame: json = {
            "role": message_role_name(message.role()),
            "content": message_text(message),
          };
          wire.push(frame);
        },
      }
    }
    root.ai.ConversationImport {
      conversation: Conversation {
        owner: self,
        history: history,
        tools_json: [],
        wire_messages: wire,
      },
      fidelity: root.ai.ConversationFidelity.MessagesOnly,
      warnings: ["message import; matching OpenAI tool-call frames were retained when available"],
    }
  }
}

implements root.ai.ResumableToolCallingProvider for Chat {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: Conversation => value,
      _ => throw baml.errors.UnknownError {
        data: null,
        message: ["wrong OpenAI conversation"],
      },
    };
    root.ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "messages": state.wire_messages,
        "tools": state.tools_json,
      }),
    }
  }

  function restore_conversation(
    self,
    token: root.ai.ConversationToken,
  ) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError {
        data: token,
        message: ["OpenAI conversation token has the wrong provider or version"],
      };
    }
    let state = baml.json.parse(token.sealed) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["invalid OpenAI conversation token"],
      },
    };
    Conversation {
      owner: self,
      history: root.ai.MessageHistory.empty(),
      tools_json: baml.json.path_or<json[]>(state, ".tools", []),
      wire_messages: baml.json.path_or<json[]>(state, ".messages", []),
    }
  }
}
