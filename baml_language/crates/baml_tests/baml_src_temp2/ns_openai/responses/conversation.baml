implements root.ai.ConversationImportProvider for OpenAIProvider {
  function import_messages<T>(
    self,
    messages: root.ai.Messages,
  ) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        self.tool_mode,
        "OpenAI prompt tool conversations cannot be reconstructed from portable messages",
      );
    }
    let request = root.openai.internal._provider_base_request(self);
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
        declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
        input: input,
        previous_response_id: null,
        request_url: request.url,
        headers: root.openai.internal._headers_without_auth(request.headers),
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

implements root.ai.ResumableAgentProvider for OpenAIProvider {
  function save_conversation(
    self,
    conversation: root.ai.Conversation,
  ) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        self.tool_mode,
        "OpenAI prompt tool conversations cannot be saved",
      );
    }
    let state = match (conversation) {
      let value: root.openai.internal.Conversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "wrong OpenAI Responses conversation",
      ),
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "OpenAI Responses cannot save a conversation owned by another provider instance",
      )
    }
    let provider_identity = baml.json.stringify({
      "model": self.model,
      "base_url": self.base_url,
      "output_mode": self.output_mode.to_string(),
      "tool_mode": self.tool_mode.to_string(),
      "parallel_tool_calls": self.parallel_tool_calls,
    });
    root.ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: baml.json.stringify({
        "model": self.model,
        "provider_identity": provider_identity,
        "output_type": state.declared_output_type,
        "history": root.openai.internal._conversation_history_json(state.history),
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
    if (self.tool_mode == root.openai.ToolMode.Prompt) {
      throw baml.errors.UnknownError.with_message<never>(
        self.tool_mode,
        "OpenAI prompt tool conversations cannot be restored",
      );
    }
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
    let token_model = baml.json.path_or(state, ".model", self.model);
    if (token_model != self.model) {
      throw baml.errors.UnknownError.with_message<never>(
        token_model,
        "OpenAI Responses conversation token belongs to another model",
      );
    }
    let provider_identity = baml.json.stringify({
      "model": self.model,
      "base_url": self.base_url,
      "output_mode": self.output_mode.to_string(),
      "tool_mode": self.tool_mode.to_string(),
      "parallel_tool_calls": self.parallel_tool_calls,
    });
    if (baml.json.path_or(state, ".provider_identity", "") != provider_identity) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "OpenAI Responses conversation token belongs to another provider configuration",
      )
    }
    let output_type = baml.json.path_or(state, ".output_type", "");
    if (output_type.length() == 0) {
      throw baml.errors.UnknownError.with_message<never>(
        token,
        "OpenAI Responses conversation token has no output type",
      );
    }
    let request = root.openai.internal._provider_base_request(self);
    root.openai.internal.Conversation {
      owner: self,
      history: root.openai.internal._conversation_history_from_json(
        baml.json.path_or<json[]>(state, ".history", []),
      ),
      declared_output_type: output_type,
      input: baml.json.path_or<json[]>(state, ".input", []),
      previous_response_id: baml
        .json
        .path_or<string?>(state, ".previous_response_id", null),
      request_url: request.url,
      headers: root.openai.internal._headers_without_auth(request.headers),
      active_tools: baml.json.path_or<json[]>(state, ".active_tools", []),
      last_output: baml.json.path_or<json[]>(state, ".last_output", []),
    }
  }
}
