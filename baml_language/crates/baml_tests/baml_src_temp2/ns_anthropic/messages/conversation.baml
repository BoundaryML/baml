implements root.ai.ConversationImportProvider for AnthropicProvider {
  function import_messages<T>(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot import messages with ToolMode.Prompt because prompt tools require the original Task and its prompt-rendering function. Use ToolMode.Native, or start a new Task from the messages.",
      );
    }
    let history = messages.to_history();
    let base_request = root.anthropic.internal._provider_base_request(self);
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
        declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
        system: system,
        messages: wire_messages,
        request_url: base_request.url,
        headers: root.anthropic.internal._headers_without_auth(base_request.headers),
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

implements root.ai.ResumableAgentProvider for AnthropicProvider {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot save a ToolMode.Prompt conversation because resuming it requires the original Task and its prompt-rendering function. Use ToolMode.Native for save and restore.",
      );
    }
    let state = match (conversation) {
      let value: root.anthropic.internal.Conversation => value,
      _ => {
        throw baml.errors.UnknownError.with_message<never>(
          null,
          "wrong Anthropic conversation",
        )
      },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Anthropic cannot save a conversation owned by another provider instance",
      )
    }
    let provider_identity = baml.json.stringify({
      "model": self.model,
      "base_url": self.base_url,
      "anthropic_version": self.anthropic_version,
      "output_mode": self.output_mode.to_string(),
      "tool_mode": self.tool_mode.to_string(),
      "strict_tool_inputs": self.strict_tool_inputs,
      "parallel_tool_calls": self.parallel_tool_calls,
    });
    root.ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "model": self.model,
        "provider_identity": provider_identity,
        "output_type": state.declared_output_type,
        "history": root.anthropic.internal._conversation_history_json(state.history),
        "system": state.system,
        "messages": state.messages,
        "active_tools": state.active_tools,
        "last_assistant_content": state.last_assistant_content,
      }),
    }
  }

  function restore_conversation(
    self,
    token: root.ai.ConversationToken,
  ) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (self.tool_mode == root.anthropic.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "Cannot restore a conversation with ToolMode.Prompt because the token does not contain the original Task or its prompt-rendering function. Use ToolMode.Native for save and restore.",
      );
    }
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
    let token_model = baml.json.path_or(state, ".model", self.model);
    if (token_model != self.model) {
      throw baml.errors.UnknownError.with_message<never>(
        token_model,
        "Anthropic conversation token belongs to another model",
      );
    }
    let provider_identity = baml.json.stringify({
      "model": self.model,
      "base_url": self.base_url,
      "anthropic_version": self.anthropic_version,
      "output_mode": self.output_mode.to_string(),
      "tool_mode": self.tool_mode.to_string(),
      "strict_tool_inputs": self.strict_tool_inputs,
      "parallel_tool_calls": self.parallel_tool_calls,
    });
    if (baml.json.path_or(state, ".provider_identity", "") != provider_identity) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "Anthropic conversation token belongs to another provider configuration",
      )
    }
    let output_type = baml.json.path_or(state, ".output_type", "");
    if (output_type.length() == 0) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "Anthropic conversation token has no output type",
      );
    }
    let base_request = root.anthropic.internal._provider_base_request(self);
    root.anthropic.internal.Conversation {
      owner: self,
      history: root.anthropic.internal._conversation_history_from_json(
        baml.json.path_or<json[]>(state, ".history", []),
      ),
      declared_output_type: output_type,
      system: baml.json.path_or<json[]>(state, ".system", []),
      messages: baml.json.path_or<json[]>(state, ".messages", []),
      request_url: base_request.url,
      headers: root.anthropic.internal._headers_without_auth(base_request.headers),
      active_tools: baml.json.path_or<json[]>(state, ".active_tools", []),
      last_assistant_content: baml.json.path_or<json[]>(
        state,
        ".last_assistant_content",
        [],
      ),
    }
  }
}
