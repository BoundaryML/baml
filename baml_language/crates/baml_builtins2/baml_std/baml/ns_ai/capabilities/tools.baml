// The Tools capability (plan Phase 3, scenario 09): a provider that runs a multi-turn
// function-calling loop. The provider-owned `Transcript` hides the wire threading (call
// ids, assistant-turn preservation, tool-result ordering); the loop speaks a tiny
// vocabulary (`ToolCall`/`ToolResult`/`ToolCalls`).

class Tool {
    name: string,
    description: string,
    parameters: string, // JSON Schema for the tool's arguments (the wire form)

    /// Build a tool whose parameter schema is lowered from a BAML `type` — the
    /// design-true typed entry (plan P7/D6): declare an args class, hand its `type`
    /// here, and SAP-parse `ToolCall.args` back into it in your dispatcher.
    ///
    ///   class WeatherArgs { city: string }
    ///   Tool.from_type("get_weather", "…", reflect.type_of<WeatherArgs>())
    function from_type(name: string, description: string, params: type) -> Tool
        throws root.errors.Unsupported {
        Tool {
            name: name,
            description: description,
            parameters: root.schema.json_schema(params, false),
        }
    }
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

//baml:llm_capability
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

//baml:llm_companion(run_tools)
/// Drive the default agentic loop (the generated `Foo$run_tools` companion
/// delegates here). Tool-calling has no honest single-call degrade — a
/// non-Tools provider is a typed `Unsupported`. The prompt is flattened to
/// text because `Tools.begin` speaks the string form.
function drive_run_tools<T>(
    client: Provider,
    prompt: root.llm.PromptAst,
    tools: Tool[],
    dispatch: (ToolCall[]) -> ToolResult[] throws never,
) -> T throws root.errors.ToolError | root.errors.UnknownError {
    let text = root.llm.prompt_to_text(prompt);
    match (client) {
        let tp: Tools => tp.run_tools<T>(text, tools, dispatch),
        _ => throw root.errors.Unsupported {
            message: "client's provider does not support Tools",
        },
    }
}
