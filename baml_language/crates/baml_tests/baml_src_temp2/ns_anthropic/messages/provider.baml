// How Anthropic returns the task's typed value.
enum OutputMode {
    // Use Anthropic's strict JSON-schema output format.
    Strict,
    // Ask for text and recover T with BAML's schema-aware parser.
    Sap,
}

// How an agent exposes application tools to Anthropic.
enum ToolMode {
    // Send application tools through the Messages tool-use API.
    Native,
    // Put T | ToolCalls and the tool catalog in the prompt. This requires
    // OutputMode.Sap, and Anthropic receives no native tools.
    Prompt,
}

// Anthropic's Messages API adapter. Initial requests are rendered through the
// primitive client so BAML media becomes Anthropic image/audio/document blocks.
class AnthropicProvider {
    model: string,
    api_key: string?,
    base_url: string?,
    max_tokens: int,
    anthropic_version: string,
    extra_headers: map<string, string>?,
    extra_body: map<string, unknown>?,
    output_mode: OutputMode,
    tool_mode: ToolMode,
    // Anthropic validates each application tool's input against its schema.
    strict_tool_inputs: bool,
    // Allow one model turn to request multiple independent application tools.
    // This is unrelated to whether T itself is an array.
    parallel_tool_calls: bool,
}

implements ai.Provider for AnthropicProvider {
  function name(self) -> string throws never { "anthropic" }

  function render_shorthand(self) -> string throws never {
    "anthropic/" + self.model
  }
}

implements ai.StreamingProvider for AnthropicProvider {
  function stream<TPartial, T>(
    self,
    task: ai.Task<T>,
  ) -> baml.llm.Stream<TPartial, T> throws baml.errors.UnknownError {
    // The primitive owns SSE accumulation. The adapter still applies the
    // selected Strict or SAP wire configuration before opening the stream.
    root.anthropic.internal._messages_stream<TPartial, T>(self, task)
  }
}

function messages(
    model: string = "claude-sonnet-4-6",
    api_key: string? = null,
    base_url: string? = null,
    max_tokens: int = 8192,
    anthropic_version: string = "2023-06-01",
    extra_headers: map<string, string>? = null,
    extra_body: map<string, unknown>? = null,
    output_mode: OutputMode = OutputMode.Strict,
    tool_mode: ToolMode = ToolMode.Native,
    strict_tool_inputs: bool = false,
    parallel_tool_calls: bool = true,
) -> AnthropicProvider throws never {
    AnthropicProvider {
        model: model,
        api_key: api_key,
        base_url: base_url,
        max_tokens: max_tokens,
        anthropic_version: anthropic_version,
        extra_headers: extra_headers,
        extra_body: extra_body,
        output_mode: output_mode,
        tool_mode: tool_mode,
        strict_tool_inputs: strict_tool_inputs,
        parallel_tool_calls: parallel_tool_calls,
    }
}
