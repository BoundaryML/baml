class BepV2ScriptedTranscript {
  turn: int,
  call_name: string?,
  call_args: string,
  final_json: string,
}

// Shared deterministic Tools provider for scenarios 09–14.
class BepV2ScriptedTools {
  call_name: string?,
  call_args: string,
  final_json: string,

  implements root.ai.Provider {}

  implements root.ai.Tools {
    type Transcript = BepV2ScriptedTranscript

    function begin<T>(self, request: root.ai.Request<T>) -> BepV2ScriptedTranscript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      BepV2ScriptedTranscript {
        turn: 0,
        call_name: self.call_name,
        call_args: self.call_args,
        final_json: self.final_json,
      }
    }

    function step<T>(self, transcript: BepV2ScriptedTranscript) -> T | root.ai.ToolCalls
        throws baml.errors.ToolError | baml.errors.UnknownError {
      match (transcript.call_name) {
        let name: string => {
          if (transcript.turn == 0) {
            return root.ai.ToolCalls {
              calls: [root.ai.ToolCall { id: "call-1", name: name, args: transcript.call_args }],
            };
          }
        },
        null => {},
      }
      baml.sap.parse<T>(transcript.final_json) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["scripted final value parse failed"] },
      }
    }

    function submit(
      self,
      transcript: BepV2ScriptedTranscript,
      results: root.ai.ToolResult[],
    ) -> BepV2ScriptedTranscript throws baml.errors.ToolError | baml.errors.UnknownError {
      BepV2ScriptedTranscript {
        turn: transcript.turn + 1,
        call_name: transcript.call_name,
        call_args: transcript.call_args,
        final_json: transcript.final_json,
      }
    }
  }
}

class S09WeatherArgs {
  city: string,
}

function s09_weather_tool() -> root.ai.Tool throws baml.errors.Unsupported {
  root.ai.Tool.typed<S09WeatherArgs>(
    "get_weather",
    "Get the current weather for a city.",
  )
}

function s09_weather_dispatch(calls: root.ai.ToolCall[]) -> root.ai.ToolResult[] throws never {
  calls.map((call: root.ai.ToolCall) -> root.ai.ToolResult {
    root.ai.invoke_tool<S09WeatherArgs>(call, (args: S09WeatherArgs) -> string {
      `cloudy, 18 C in ${args.city}`
    })
  })
}
