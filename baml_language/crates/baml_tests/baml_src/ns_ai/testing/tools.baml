class FixtureToolTranscript {
  turn: int,
  call_name: string?,
  call_args: string,
  final_json: string,
}

// One optional tool call followed by one exact JSON final value.
class FixtureTools {
  call_name: string?,
  call_args: string,
  final_json: string,

  implements Provider {
    function display_name(self) -> string throws never { "fixture-tools" }
  }

  implements Tools {
    type Transcript = FixtureToolTranscript

    function begin<T>(self, request: Request<T>) -> FixtureToolTranscript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      FixtureToolTranscript {
        turn: 0,
        call_name: self.call_name,
        call_args: self.call_args,
        final_json: self.final_json,
      }
    }

    function step<T>(self, transcript: FixtureToolTranscript) -> T | ToolCalls
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
          message: ["fixture final value did not match the requested output type"],
        },
      }
    }

    function submit(
      self,
      transcript: FixtureToolTranscript,
      results: ToolResult[],
    ) -> FixtureToolTranscript throws baml.errors.ToolError | baml.errors.UnknownError {
      FixtureToolTranscript {
        turn: transcript.turn + 1,
        call_name: transcript.call_name,
        call_args: transcript.call_args,
        final_json: transcript.final_json,
      }
    }
  }
}
