implements root.ai.tools.ConversationImportProvider for Responses {
  function import_messages(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws never {
    let history = messages.to_history();
    let input: json[] = [];
    for (let message in history.entries) {
      let item: json = {
        "role": root.openai.internal._message_role_name(message.role()),
        "content": root.openai.internal._message_text(message),
      };
      input.push(item);
    }
    root.ai.ConversationImport {
      conversation: root.openai.internal.Conversation {
        owner: self,
        history: history,
        input: input,
        previous_response_id: null,
        active_tools: [],
        last_output: [],
      },
      fidelity: root.ai.ConversationFidelity.MessagesOnly,
      warnings: [
        "message import does not retain OpenAI response IDs or opaque reasoning items",
      ],
    }
  }
}

implements root.ai.tools.ResumableToolCallingProvider for Responses {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: root.openai.internal.Conversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "wrong OpenAI Responses conversation",
      ),
    };
    root.ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "input": state.input,
        "previous_response_id": state.previous_response_id,
        "active_tools": state.active_tools,
        "last_output": state.last_output,
      }),
    }
  }

  function restore_conversation(
    self,
    token: root.ai.ConversationToken,
  ) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (token.provider != self.name() || token.version != 1) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "OpenAI Responses conversation token has the wrong provider or version",
      );
    }
    let state = baml.json.parse(token.sealed) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(
        e,
        "invalid OpenAI Responses conversation token",
      ),
    };
    root.openai.internal.Conversation {
      owner: self,
      history: root.ai.MessageHistory.empty(),
      input: baml.json.path_or<json[]>(state, ".input", []),
      previous_response_id: baml
        .json
        .path_or<string?>(state, ".previous_response_id", null),
      active_tools: baml.json.path_or<json[]>(state, ".active_tools", []),
      last_output: baml.json.path_or<json[]>(state, ".last_output", []),
    }
  }
}
