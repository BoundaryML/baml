class FakeToolConversation {
    owner: FakeToolProvider,
    history: root.ai.MessageHistory,
    turn: int,
    last_tools: root.ai.tools.Tool[],
    last_provider_tools: root.ai.tools.ProviderTool[],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
    }
}

class FakeToolProvider {
    provider_name: string,
    tool_calls: root.ai.tools.ToolCall[],
    final_output: string,
    provider_tools: root.ai.tools.ProviderTool[],
}

implements root.ai.Provider for FakeToolProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements root.ai.GenerationProvider for FakeToolProvider {
  function generate<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T> throws baml.errors.UnknownError {
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "fake tool final parse failed"),
    };
    root.ai.ResponseWithMetadata<T> {
      value: value,
      metadata: root.ai.ResponseMetadata {
        provider: self.provider_name, model: "fake", request_id: null,
        finish_reason: "stop", usage: null, attributes: {}, raw: null,
      },
      conversation: null,
    }
  }
}

implements root.ai.tools.ToolCallingProvider for FakeToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation throws never {
    FakeToolConversation {
      owner: self,
      history: task.messages().to_history(),
      turn: 0,
      last_tools: task.tools,
      last_provider_tools: self.provider_tools,
    }
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.tools.Tool[]) -> root.ai.tools.ModelStep<T>
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong fake conversation"),
    };
    state.last_tools = tools;
    state.last_provider_tools = self.provider_tools;
    if (state.turn == 0 && self.tool_calls.length() > 0) {
      state.turn = 1;
      return root.ai.tools.ModelStep<T> {
        outcome: root.ai.tools.ToolCalls { calls: self.tool_calls },
        metadata: _fake_tool_step_meta(self.name(), state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "fake tool final parse failed"),
    };
    root.ai.tools.ModelStep<T> {
      outcome: value,
      metadata: _fake_tool_step_meta(self.name(), state.turn + 1),
    }
  }

  function submit(self, conversation: root.ai.Conversation, results: root.ai.tools.ToolResult[]) -> root.ai.Conversation throws never {
    conversation
  }
}

implements root.ai.tools.ConversationImportProvider for FakeToolProvider {
  function import_messages(self, messages: root.ai.Messages) -> root.ai.ConversationImport throws never {
    root.ai.ConversationImport {
      conversation: FakeToolConversation {
        owner: self,
        history: messages.to_history(),
        turn: 0,
        last_tools: [],
        last_provider_tools: self.provider_tools,
      },
      fidelity: root.ai.ConversationFidelity.MessagesOnly,
      warnings: [],
    }
  }
}

class ScriptedToolConversation {
    owner: ScriptedToolProvider,
    history: root.ai.MessageHistory,
    turn: int,
    observed_tool_rosters: string[][],
    submitted_results: root.ai.tools.ToolResult[][],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
    }
}

class ScriptedToolProvider {
    turns: root.ai.tools.ToolCalls[],
    final_output: string,
    usage_per_step: root.ai.Usage?,
}

implements root.ai.Provider for ScriptedToolProvider {
  function name(self) -> string throws never { "scripted-tools" }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements root.ai.tools.ToolCallingProvider for ScriptedToolProvider {
  function begin<T>(self, task: root.ai.Task<T>) -> root.ai.Conversation throws never {
    ScriptedToolConversation {
      owner: self,
      history: task.messages().to_history(),
      turn: 0,
      observed_tool_rosters: [],
      submitted_results: [],
    }
  }

  function step<T>(self, conversation: root.ai.Conversation, tools: root.ai.tools.Tool[]) -> root.ai.tools.ModelStep<T>
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong scripted tool conversation"),
    };
    state.observed_tool_rosters.push(
      tools.map((tool: root.ai.tools.Tool) -> string { tool.name }),
    );
    if (state.turn < self.turns.length()) {
      let calls = self.turns[state.turn];
      state.turn = state.turn + 1;
      return root.ai.tools.ModelStep<T> {
        outcome: calls,
        metadata: _scripted_tool_step_meta(self, state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "scripted tool final parse failed"),
    };
    root.ai.tools.ModelStep<T> {
      outcome: value,
      metadata: _scripted_tool_step_meta(self, state.turn + 1),
    }
  }

  function submit(self, conversation: root.ai.Conversation, results: root.ai.tools.ToolResult[]) -> root.ai.Conversation
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(null, "wrong scripted tool conversation"),
    };
    state.submitted_results.push(results);
    state
  }
}

function _fake_tool_step_meta(provider: string, turn: int) -> root.ai.ResponseMetadata throws never {
    root.ai.ResponseMetadata {
        provider: provider,
        model: "fake",
        request_id: "fake-tool-" + turn.to_string(),
        finish_reason: null,
        usage: root.ai.Usage {
            input_tokens: 1,
            output_tokens: 1,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            cost_usd: 0.0,
        },
        attributes: {},
        raw: null,
    }
}

function _scripted_tool_step_meta(
    provider: ScriptedToolProvider,
    turn: int,
) -> root.ai.ResponseMetadata throws never {
    root.ai.ResponseMetadata {
        provider: provider.name(),
        model: "fake",
        request_id: "scripted-tool-" + turn.to_string(),
        finish_reason: null,
        usage: provider.usage_per_step,
        attributes: {},
        raw: null,
    }
}
