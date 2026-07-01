// The Tools capability (plan Phase 3, scenario 09): a provider that runs a multi-turn
// function-calling loop. The provider-owned `Transcript` hides the wire threading (call
// ids, assistant-turn preservation, tool-result ordering); the loop speaks a tiny
// vocabulary (`ToolCall`/`ToolResult`/`ToolCalls`).
//
// NOTE (deviation, see _plan/deviations.md): `Tool.parameters` is a JSON-Schema *string*
// (app-provided), because a generic `type -> JSON Schema` lowering host fn does not exist
// yet. The design's `parameters: type` + `type_to_json_schema` is a follow-up.

class Tool {
    name: string,
    description: string,
    parameters: string, // JSON Schema for the tool's arguments
}

/// One tool invocation the model requested. `id` is opaque to the loop; `args` is the
/// raw JSON arguments string the provider emitted.
class ToolCall {
    id: string,
    name: string,
    args: string,
}

/// The result the app produced for a `ToolCall` (fed back to the model). `output` is a
/// string (JSON or plain text).
class ToolResult {
    id: string,
    output: string,
}

/// Sentinel: the model asked to call tools rather than produce the final value
/// (the `Done`-shaped arm of the `step` loop).
class ToolCalls {
    calls: ToolCall[],
}

/// Multi-turn tool loop. `Transcript` is provider-owned + opaque.
interface Tools requires Provider {
    type Transcript

    function begin<T>(self, prompt: string, tools: Tool[]) -> Transcript
        throws root.errors.ToolError | root.errors.UnknownError
    // Mirror of `Iterator.next -> Item | Done`: either the final value or a request to call tools.
    function step<T>(self, t: Transcript) -> T | ToolCalls
        throws root.errors.ToolError | root.errors.UnknownError
    function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
        throws root.errors.ToolError | root.errors.UnknownError

    // The default agentic loop: begin, then step/dispatch/submit until a value falls out.
    // `dispatch` is the app's tool executor (calls -> results); it handles its own errors.
    function run_tools<T>(
        self,
        prompt: string,
        tools: Tool[],
        dispatch: (ToolCall[]) -> ToolResult[] throws never,
    ) -> T throws root.errors.ToolError | root.errors.UnknownError {
        let t = self.begin<T>(prompt, tools);
        while (true) {
            match (self.step<T>(t)) {
                let calls: ToolCalls => {
                    t = self.submit(t, dispatch(calls.calls));
                },
                let value: T => {
                    return value;
                },
            }
        }
        root.sys.panic("unreachable")
    }
}
