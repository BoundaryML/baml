class FakeToolTranscript {
  owner: FakeToolProvider,
  history: Conversation,
  turn: int,
  last_tools: Tool[],
  last_provider_tools: ProviderTool[],

  implements Transcript {
    function provider(self) -> Provider throws never { self.owner }
    function messages(self) -> Messages throws never { self.history }
    function conversation(self) -> Conversation throws never { self.history }
  }
}

class FakeToolProvider {
  tool_calls: ToolCall[],
  final_output: string,
  provider_tools: ProviderTool[],
}

implements Provider for FakeToolProvider {
  function name(self) -> string throws never { "fake-tools" }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
}

implements GenerationProvider for FakeToolProvider {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    let value = baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["fake tool final parse failed"] },
    };
    Response<T> {
      value: value,
      meta: Meta {
        provider: "fake-tools", model: "fake", request_id: null,
        finish_reason: "stop", usage: null, attributes: {}, raw: null,
      },
      transcript: null,
    }
  }
}

implements ToolCallingProvider for FakeToolProvider {
  function begin<T>(self, task: Task<T>) -> Transcript throws never {
    FakeToolTranscript {
      owner: self,
      history: task.messages().to_conversation(),
      turn: 0,
      last_tools: task.tools,
      last_provider_tools: self.provider_tools,
    }
  }

  function step<T>(self, transcript: Transcript, tools: Tool[]) -> T | ToolCalls
      throws baml.errors.UnknownError {
    let state = match (transcript) {
      let value: FakeToolTranscript => value,
      _ => throw baml.errors.UnknownError { data: null, message: ["wrong fake transcript"] },
    };
    state.last_tools = tools;
    state.last_provider_tools = self.provider_tools;
    if (state.turn == 0 && self.tool_calls.length() > 0) {
      state.turn = 1;
      return ToolCalls { calls: self.tool_calls };
    }
    baml.sap.parse<T>(self.final_output) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["fake tool final parse failed"] },
    }
  }

  function submit(self, transcript: Transcript, results: ToolResult[]) -> Transcript throws never {
    transcript
  }
}

implements TranscriptImportProvider for FakeToolProvider {
  function import_conversation(self, conversation: Conversation) -> TranscriptImport throws never {
    TranscriptImport {
      transcript: FakeToolTranscript {
        owner: self,
        history: conversation,
        turn: 0,
        last_tools: [],
        last_provider_tools: self.provider_tools,
      },
      fidelity: TranscriptFidelity.MessagesOnly,
      warnings: [],
    }
  }
}
