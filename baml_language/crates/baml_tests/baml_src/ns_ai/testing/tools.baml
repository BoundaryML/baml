class FakeToolTranscript {
  turn: int,
  call_name: string?,
  call_args: string,
  final_json: string,
}

// One optional tool call followed by one exact JSON final value.
class FakeToolProvider {
  call_name: string?,
  call_args: string,
  final_json: string,

  implements Provider {
    function display_name(self) -> string throws never { "fake-tools" }
  }

  implements Tools {
    type Transcript = FakeToolTranscript

    function begin<T>(self, request: Request<T>) -> FakeToolTranscript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      FakeToolTranscript {
        turn: 0,
        call_name: self.call_name,
        call_args: self.call_args,
        final_json: self.final_json,
      }
    }

    function step<T>(self, transcript: FakeToolTranscript) -> T | ToolCalls
        throws baml.errors.ToolError | baml.errors.UnknownError {
      match (transcript.call_name) {
        let name: string if transcript.turn == 0 => return ToolCalls {
          calls: [ToolCall { id: "call-1", name: name, args: transcript.call_args }],
        },
        _ => {},
      }
      baml.json.from_string<T>(transcript.final_json) catch (e) {
        _ => throw baml.errors.UnknownError {
          data: e,
          message: ["fake final value did not match the requested output type"],
        },
      }
    }

    function submit(
      self,
      transcript: FakeToolTranscript,
      results: ToolResult[],
    ) -> FakeToolTranscript throws baml.errors.ToolError | baml.errors.UnknownError {
      FakeToolTranscript {
        turn: transcript.turn + 1,
        call_name: transcript.call_name,
        call_args: transcript.call_args,
        final_json: transcript.final_json,
      }
    }

    function supports_dynamic_tools(self) -> bool throws never { true }
  }
}
