class FakeToolConversation {
    owner: FakeToolProvider,
    history: ai.MessageHistory,
    declared_output_type: string,
    turn: int,
    pending: ai.tools.ToolCall[],
    last_tools: ai.tools.Tool[],
    last_provider_tools: ai.tools.ProviderTool[],

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }

        function pending_calls(self) -> ai.tools.ToolCall[]? throws never {
            self.pending
        }
        function messages(self) -> ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

class FakeToolProvider {
    provider_name: string,
    tool_calls: ai.tools.ToolCall[],
    final_output: string,
    provider_tools: ai.tools.ProviderTool[],
}

implements ai.Provider for FakeToolProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements ai.AgentProvider for FakeToolProvider {
  function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
    FakeToolConversation {
      owner: self,
      history: task.messages().to_history(),
      declared_output_type: ai.internal._output_type_fingerprint<T>(),
      turn: 0,
      pending: [],
      last_tools: task.tools,
      last_provider_tools: self.provider_tools,
    }
  }

  function step<T>(self, conversation: ai.Conversation, tools: ai.tools.Tool[]) -> ai.ModelStep<T>
      throws ai.Failure | baml.errors.UnknownError {
    ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong fake conversation"),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        state.owner.name(),
        "fake tool conversation belongs to another provider instance",
      )
    }
    state.last_tools = tools;
    state.last_provider_tools = self.provider_tools;
    if (state.turn == 0 && self.tool_calls.length() > 0) {
      state.turn = 1;
      state.pending = self.tool_calls;
      return ai.ModelStep<T> {
        assistant_text: null,
        reasoning_text: null,
        outcome: ai.tools.ToolCalls { calls: self.tool_calls },
        metadata: _fake_tool_step_meta(self.name(), state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "fake tool final parse failed"),
    };
    ai.ModelStep<T> {
      assistant_text: null,
      reasoning_text: null,
      outcome: value,
      metadata: _fake_tool_step_meta(self.name(), state.turn + 1),
    }
  }

  function submit(self, conversation: ai.Conversation, results: ai.tools.ToolResult[]) -> ai.Conversation
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong fake conversation"),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        state.owner.name(),
        "fake tool conversation belongs to another provider instance",
      )
    }
    state.pending = [];
    state
  }
}

implements ai.ConversationImportProvider for FakeToolProvider {
  function import_messages<T>(self, messages: ai.Messages) -> ai.ConversationImport throws never {
    ai.ConversationImport {
      conversation: FakeToolConversation {
        owner: self,
        history: messages.to_history(),
        declared_output_type: ai.internal._output_type_fingerprint<T>(),
        turn: 0,
        pending: [],
        last_tools: [],
        last_provider_tools: self.provider_tools,
      },
      fidelity: ai.ConversationFidelity.MessagesOnly,
      warnings: [],
    }
  }
}

implements ai.ConversationAppendProvider for FakeToolProvider {
  function append_messages(
    self,
    conversation: ai.Conversation,
    messages: ai.Messages,
  ) -> null throws ai.Failure {
    let _ = ai.internal._conversation_append_user_texts(messages, self.name());
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "wrong fake tool conversation",
      },
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "fake tool conversation belongs to another provider instance",
      }
    }
    if (state.pending.length() != 0) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "submit fake tool results before appending a user message",
      };
    }
    for (let message in messages.items()) {
      state.history.entries.push(message);
    }
    state.turn = 0;
    null
  }
}

class ScriptedToolConversation {
    owner: ScriptedToolProvider,
    history: ai.MessageHistory,
    declared_output_type: string,
    turn: int,
    pending: ai.tools.ToolCall[],
    observed_tool_rosters: string[][],
    submitted_results: ai.tools.ToolResult[][],

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }

        function pending_calls(self) -> ai.tools.ToolCall[]? throws never {
            self.pending
        }
        function messages(self) -> ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

class ScriptedToolProvider {
    turns: ai.tools.ToolCalls[],
    final_output: string,
    usage_per_step: ai.Usage?,
}

implements ai.Provider for ScriptedToolProvider {
  function name(self) -> string throws never { "scripted-tools" }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements ai.AgentProvider for ScriptedToolProvider {
  function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
    ScriptedToolConversation {
      owner: self,
      history: task.messages().to_history(),
      declared_output_type: ai.internal._output_type_fingerprint<T>(),
      turn: 0,
      pending: [],
      observed_tool_rosters: [],
      submitted_results: [],
    }
  }

  function step<T>(self, conversation: ai.Conversation, tools: ai.tools.Tool[]) -> ai.ModelStep<T>
      throws ai.Failure | baml.errors.UnknownError {
    ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong scripted tool conversation"),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        state.owner.name(),
        "scripted tool conversation belongs to another provider instance",
      )
    }
    state.observed_tool_rosters.push(
      tools.map((tool: ai.tools.Tool) -> string { tool.name }),
    );
    if (state.turn < self.turns.length()) {
      let calls = self.turns[state.turn];
      state.turn = state.turn + 1;
      state.pending = calls.calls;
      return ai.ModelStep<T> {
        assistant_text: null,
        reasoning_text: null,
        outcome: calls,
        metadata: _scripted_tool_step_meta(self, state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "scripted tool final parse failed"),
    };
    ai.ModelStep<T> {
      assistant_text: null,
      reasoning_text: null,
      outcome: value,
      metadata: _scripted_tool_step_meta(self, state.turn + 1),
    }
  }

  function submit(self, conversation: ai.Conversation, results: ai.tools.ToolResult[]) -> ai.Conversation
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong scripted tool conversation"),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        state.owner.name(),
        "scripted tool conversation belongs to another provider instance",
      )
    }
    state.pending = [];
    state.submitted_results.push(results);
    state
  }
}

implements ai.ConversationAppendProvider for ScriptedToolProvider {
  function append_messages(
    self,
    conversation: ai.Conversation,
    messages: ai.Messages,
  ) -> null throws ai.Failure {
    let _ = ai.internal._conversation_append_user_texts(messages, self.name());
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "wrong scripted tool conversation",
      },
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "scripted tool conversation belongs to another provider instance",
      }
    }
    if (state.pending.length() != 0) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "submit scripted tool results before appending a user message",
      };
    }
    for (let message in messages.items()) {
      state.history.entries.push(message);
    }
    state.turn = 0;
    null
  }
}

function _fake_tool_step_meta(
    provider: string,
    turn: int,
) -> ai.ResponseMetadata throws never {
    ai.ResponseMetadata {
        provider: provider,
        model: "fake",
        request_id: "fake-tool-" + turn.to_string(),
        finish_reason: null,
        usage: ai.Usage {
            input_tokens: 1,
            output_tokens: 1,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
        },
        attributes: {},
        raw: null,
    }
}

function _scripted_tool_step_meta(
    provider: ScriptedToolProvider,
    turn: int,
) -> ai.ResponseMetadata throws never {
    ai.ResponseMetadata {
        provider: provider.name(),
        model: "fake",
        request_id: "scripted-tool-" + turn.to_string(),
        finish_reason: null,
        usage: provider.usage_per_step,
        attributes: {},
        raw: null,
    }
}

// ---------------------------------------------------------------------------
// Save/restore for fake conversations. Fakes model provider capabilities, and
// sessions fork through save/restore — so the fakes serialize their state
// with text fidelity, which is exactly what they model.
// ---------------------------------------------------------------------------

class _FakeSealedMessage {
    role: string,
    text: string,
}

class _FakeSealedState {
    declared_output_type: string,
    turn: int,
    had_pending: bool,
    messages: _FakeSealedMessage[],
}

function _fake_seal(
    history: ai.MessageHistory,
    declared_output_type: string,
    turn: int,
    had_pending: bool,
) -> string throws baml.errors.UnknownError {
    let messages = history.entries.map((message: ai.Message) -> _FakeSealedMessage {
        _FakeSealedMessage {
            role: message.role().to_string(),
            text: ai.message_text(message),
        }
    });
    let sealed = baml.json.to_json(_FakeSealedState {
        declared_output_type: declared_output_type,
        turn: turn,
        had_pending: had_pending,
        messages: messages,
    }) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(e, "fake conversation seal"),
    };
    baml.json.stringify(sealed)
}

function _fake_unseal(sealed: string) -> _FakeSealedState throws baml.errors.UnknownError {
    baml.json.from_string<_FakeSealedState>(sealed) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(e, "fake conversation restore"),
    }
}

function _fake_role(name: string) -> ai.MessageRole throws never {
    if (name.ends_with("System")) {
        ai.MessageRole.System
    } else if (name.ends_with("Assistant")) {
        ai.MessageRole.Assistant
    } else if (name.ends_with("Tool")) {
        ai.MessageRole.Tool
    } else {
        ai.MessageRole.User
    }
}

function _fake_history(messages: _FakeSealedMessage[]) -> ai.MessageHistory throws never {
    ai.MessageHistory {
        entries: messages.map((message: _FakeSealedMessage) -> ai.Message {
            ai.ChatMessage {
                message_role: _fake_role(message.role),
                content: [ai.messages.TextPart { value: message.text, metadata: {} }],
                metadata: null,
            }
        }),
    }
}

implements ai.ResumableAgentProvider for FakeToolProvider {
  function save_conversation(
    self,
    conversation: ai.Conversation,
  ) -> ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong fake tool conversation"),
    };
    ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: _fake_seal(state.history, state.declared_output_type, state.turn, state.pending.length() != 0),
    }
  }

  function restore_conversation(
    self,
    token: ai.ConversationToken,
  ) -> ai.Conversation throws baml.errors.UnknownError {
    let sealed = _fake_unseal(token.sealed);
    FakeToolConversation {
      owner: self,
      history: _fake_history(sealed.messages),
      declared_output_type: sealed.declared_output_type,
      turn: sealed.turn,
      pending: [],
      last_tools: [],
      last_provider_tools: [],
    }
  }
}

implements ai.ResumableAgentProvider for ScriptedToolProvider {
  function save_conversation(
    self,
    conversation: ai.Conversation,
  ) -> ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong scripted tool conversation"),
    };
    ai.ConversationToken {
      provider: self.name(),
      version: 1,
      sealed: _fake_seal(state.history, state.declared_output_type, state.turn, state.pending.length() != 0),
    }
  }

  function restore_conversation(
    self,
    token: ai.ConversationToken,
  ) -> ai.Conversation throws baml.errors.UnknownError {
    let sealed = _fake_unseal(token.sealed);
    ScriptedToolConversation {
      owner: self,
      history: _fake_history(sealed.messages),
      declared_output_type: sealed.declared_output_type,
      turn: sealed.turn,
      pending: [],
      observed_tool_rosters: [],
      submitted_results: [],
    }
  }
}
