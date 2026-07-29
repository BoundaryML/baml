implements root.ai.tools.ConversationImportProvider for Messages {
  function import_messages(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws never {
    let history = messages.to_history();
    let system: json[] = [];
    let wire_messages: json[] = [];
    for (let message in history.entries) {
      let text = root.anthropic.internal._message_text(message);
      if (message.role() == root.ai.MessageRole.System) {
        let system_part: json = { "type": "text", "text": text };
        system.push(system_part);
      } else {
        let wire_message: json = {
          "role": root.anthropic.internal._message_role_name(message.role()),
          "content": [{ "type": "text", "text": text }],
        };
        wire_messages.push(wire_message);
      }
    }
    root.ai.ConversationImport {
      conversation: root.anthropic.internal.Conversation {
        owner: self,
        history: history,
        system: system,
        messages: wire_messages,
        request_url: root.anthropic.internal._provider_url(self),
        active_tools: [],
        last_assistant_content: [],
      },
      fidelity: root.ai.ConversationFidelity.MessagesOnly,
      warnings: [
        "message import keeps text only; media, tool IDs, and opaque thinking signatures are not reconstructed",
      ],
    }
  }
}

implements root.ai.tools.ResumableToolCallingProvider for Messages {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: root.anthropic.internal.Conversation => value,
      _ => {
        throw baml.errors.UnknownError.with_message<never>(
          null,
          "wrong Anthropic conversation",
        )
      },
    };
    root.ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "system": state.system,
        "messages": state.messages,
        "request_url": state.request_url,
        "active_tools": state.active_tools,
        "last_assistant_content": state.last_assistant_content,
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
        "Anthropic conversation token has the wrong provider or version",
      );
    }
    let state = baml.json.parse(token.sealed) catch (e) {
      _ => {
        throw baml.errors.UnknownError.with_message<never>(
          e,
          "invalid Anthropic conversation token",
        )
      },
    };
    root.anthropic.internal.Conversation {
      owner: self,
      history: root.ai.MessageHistory.empty(),
      system: baml.json.path_or<json[]>(state, ".system", []),
      messages: baml.json.path_or<json[]>(state, ".messages", []),
      request_url: baml.json.path_or(
        state,
        ".request_url",
        root.anthropic.internal._provider_url(self),
      ),
      active_tools: baml.json.path_or<json[]>(state, ".active_tools", []),
      last_assistant_content: baml.json.path_or<json[]>(
        state,
        ".last_assistant_content",
        [],
      ),
    }
  }
}
