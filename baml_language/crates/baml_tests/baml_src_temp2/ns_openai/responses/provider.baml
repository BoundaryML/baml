// How OpenAI returns the task's typed value.
enum OutputMode {
    // Use a non-strict Responses function tool for the final T.
    Native,
    // Use an OpenAI-strict Responses function tool for the final T.
    Strict,
    // Ask for text and recover T with BAML's schema-aware parser.
    Sap,
}

// How an agent exposes application tools to OpenAI.
enum ToolMode {
    // Send application tools through the Responses function-calling API.
    Native,
    // Put T | ToolCalls and the tool catalog in the prompt. This requires
    // OutputMode.Sap, and OpenAI sends no function tools.
    Prompt,
}

// OpenAI's Responses API adapter. The output and application-tool protocols
// are configured independently. The default uses Responses function tools for
// both. Sap + Native is hybrid; Sap + Prompt uses text for the whole agent turn.
class OpenAIProvider {
    model: string,
    api_key: string?,
    base_url: string?,
    extra_headers: map<string, string>?,
    extra_body: map<string, unknown>?,
    output_mode: OutputMode,
    tool_mode: ToolMode,
    // Allow one Responses turn to request multiple independent application
    // tools. This never depends on whether T itself is an array.
    parallel_tool_calls: bool,
}

implements root.ai.Provider for OpenAIProvider {
  function name(self) -> string throws never { "openai" }
  function render_shorthand(self) -> string throws never {
    "openai-responses/" + self.model
  }
}

function responses(
    model: string = "gpt-5.6-luna",
    api_key: string? = null,
    base_url: string? = null,
    extra_headers: map<string, string>? = null,
    extra_body: map<string, unknown>? = null,
    output_mode: OutputMode = OutputMode.Native,
    tool_mode: ToolMode = ToolMode.Native,
    parallel_tool_calls: bool = true,
) -> OpenAIProvider throws never {
    OpenAIProvider {
        model: model,
        api_key: api_key,
        base_url: base_url,
        extra_headers: extra_headers,
        extra_body: extra_body,
        output_mode: output_mode,
        tool_mode: tool_mode,
        parallel_tool_calls: parallel_tool_calls,
    }
}
