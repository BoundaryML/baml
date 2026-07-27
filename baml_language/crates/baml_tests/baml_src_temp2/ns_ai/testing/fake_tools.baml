class FakeToolConversation {
    owner: FakeToolProvider,
    history: MessageHistory,
    turn: int,
    last_tools: Tool[],
    last_provider_tools: ProviderTool[],

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function messages(self) -> Messages throws never {
            self.history
        }
    }
}

class FakeToolProvider {
    provider_name: string,
    tool_calls: ToolCall[],
    final_output: string,
    provider_tools: ProviderTool[],
}

implements Provider for FakeToolProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { fake_render_shorthand() }
}

implements GenerationProvider for FakeToolProvider {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["fake tool final parse failed"] },
    };
    Response<T> {
      value: value,
      meta: Meta {
        provider: self.provider_name, model: "fake", request_id: null,
        finish_reason: "stop", usage: null, attributes: {}, raw: null,
      },
      conversation: null,
    }
  }
}

implements ToolCallingProvider for FakeToolProvider {
  function begin<T>(self, task: Task<T>) -> Conversation throws never {
    FakeToolConversation {
      owner: self,
      history: task.messages().to_history(),
      turn: 0,
      last_tools: task.tools,
      last_provider_tools: self.provider_tools,
    }
  }

  function step<T>(self, conversation: Conversation, tools: Tool[]) -> ModelStep<T>
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: FakeToolConversation => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["wrong fake conversation"] },
    };
    state.last_tools = tools;
    state.last_provider_tools = self.provider_tools;
    if (state.turn == 0 && self.tool_calls.length() > 0) {
      state.turn = 1;
      return ModelStep<T> {
        outcome: ToolCalls { calls: self.tool_calls },
        meta: fake_tool_step_meta(self.name(), state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["fake tool final parse failed"] },
    };
    ModelStep<T> {
      outcome: value,
      meta: fake_tool_step_meta(self.name(), state.turn + 1),
    }
  }

  function submit(self, conversation: Conversation, results: ToolResult[]) -> Conversation throws never {
    conversation
  }
}

implements ConversationImportProvider for FakeToolProvider {
  function import_messages(self, messages: Messages) -> ConversationImport throws never {
    ConversationImport {
      conversation: FakeToolConversation {
        owner: self,
        history: messages.to_history(),
        turn: 0,
        last_tools: [],
        last_provider_tools: self.provider_tools,
      },
      fidelity: ConversationFidelity.MessagesOnly,
      warnings: [],
    }
  }
}

class ScriptedToolConversation {
    owner: ScriptedToolProvider,
    history: MessageHistory,
    turn: int,
    observed_tool_rosters: string[][],
    submitted_results: ToolResult[][],

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }
        function messages(self) -> Messages throws never {
            self.history
        }
    }
}

class ScriptedToolProvider {
    turns: ToolCalls[],
    final_output: string,
    usage_per_step: Usage?,
}

implements Provider for ScriptedToolProvider {
  function name(self) -> string throws never { "scripted-tools" }
  function render_shorthand(self) -> string throws never { fake_render_shorthand() }
}

implements ToolCallingProvider for ScriptedToolProvider {
  function begin<T>(self, task: Task<T>) -> Conversation throws never {
    ScriptedToolConversation {
      owner: self,
      history: task.messages().to_history(),
      turn: 0,
      observed_tool_rosters: [],
      submitted_results: [],
    }
  }

  function step<T>(self, conversation: Conversation, tools: Tool[]) -> ModelStep<T>
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError {
        data: null,
        message: ["wrong scripted tool conversation"],
      },
    };
    state.observed_tool_rosters.push(
      tools.map((tool: Tool) -> string { tool.name }),
    );
    if (state.turn < self.turns.length()) {
      let calls = self.turns[state.turn];
      state.turn = state.turn + 1;
      return ModelStep<T> {
        outcome: calls,
        meta: scripted_tool_step_meta(self, state.turn),
      };
    }
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError {
        data: e,
        message: ["scripted tool final parse failed"],
      },
    };
    ModelStep<T> {
      outcome: value,
      meta: scripted_tool_step_meta(self, state.turn + 1),
    }
  }

  function submit(self, conversation: Conversation, results: ToolResult[]) -> Conversation
      throws baml.errors.UnknownError {
    let state = match (conversation) {
      let value: ScriptedToolConversation => value,
      _ => throw baml.errors.UnknownError {
        data: null,
        message: ["wrong scripted tool conversation"],
      },
    };
    state.submitted_results.push(results);
    state
  }
}

function fake_tool_step_meta(provider: string, turn: int) -> Meta throws never {
    Meta {
        provider: provider,
        model: "fake",
        request_id: "fake-tool-" + turn.to_string(),
        finish_reason: null,
        usage: Usage {
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

function scripted_tool_step_meta(provider: ScriptedToolProvider, turn: int) -> Meta throws never {
    Meta {
        provider: provider.name(),
        model: "fake",
        request_id: "scripted-tool-" + turn.to_string(),
        finish_reason: null,
        usage: provider.usage_per_step,
        attributes: {},
        raw: null,
    }
}
