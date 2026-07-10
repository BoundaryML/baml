// 24 — the live/tool-capable provider owns transcript threading; the app owns dispatch.
class s24_Transcript { called: bool, output: string }

class s24_LiveTools {
  implements root.ai.Provider { function display_name(self) -> string throws never { "live-tools-fixture" } }
  implements root.ai.Tools {
    type Transcript = s24_Transcript
    function begin<T>(self, request: root.ai.Request<T>) -> s24_Transcript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      s24_Transcript { called: false, output: "" }
    }
    function step<T>(self, transcript: s24_Transcript) -> T | root.ai.ToolCalls
        throws baml.errors.UnknownError {
      if (!transcript.called) {
        return root.ai.ToolCalls { calls: [root.ai.ToolCall {
          id: "weather-1", name: "get_weather", args: `{"city":"Paris"}`,
        }] };
      }
      baml.sap.parse<T>(`The weather is ${transcript.output}.`) catch (e) {
        _ => throw baml.errors.UnknownError { data: e, message: ["tool result parse"] },
      }
    }
    function submit(self, transcript: s24_Transcript, results: root.ai.ToolResult[]) -> s24_Transcript
        throws baml.errors.ToolError | baml.errors.UnknownError {
      s24_Transcript { called: true, output: results.at(0)?.output ?? "unknown" }
    }
  }
}
