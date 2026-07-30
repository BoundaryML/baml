// How Vertex Gemini returns the task's typed value.
enum OutputMode {
    // Use controlled JSON generation and decode the exact JSON as T.
    Native,
    // Return text and recover T with BAML's schema-aware parser.
    Sap,
}

// How an agent exposes application tools to Vertex Gemini.
enum ToolMode {
    // Use Gemini functionDeclarations and functionResponse content parts.
    Native,
    // Put T | ToolCalls in the prompt. Requires OutputMode.Sap.
    Prompt,
}

// Vertex AI Gemini adapter. Standard mode uses project/location plus ADC.
// Supplying api_key selects Vertex Express Mode, whose project-less endpoint is
// distinct from the standard regional Vertex endpoint.
class Gemini {
    model: string,
    project_id: string?,
    location: string?,
    credentials: string?,
    credentials_content: string?,
    api_key: string?,
    base_url: string?,
    extra_headers: map<string, string>?,
    extra_query: map<string, string>?,
    extra_body: map<string, unknown>?,
    output_mode: OutputMode,
    tool_mode: ToolMode,
    parallel_tool_calls: bool,
}

implements ai.Provider for Gemini {
  function name(self) -> string throws never { "google-vertex" }
  function render_shorthand(self) -> string throws never {
    "vertex-ai/" + self.model
  }
}

function gemini(
    model: string = "gemini-2.5-flash",
    project_id: string? = null,
    location: string? = null,
    credentials: string? = null,
    credentials_content: string? = null,
    api_key: string? = null,
    base_url: string? = null,
    extra_headers: map<string, string>? = null,
    extra_query: map<string, string>? = null,
    extra_body: map<string, unknown>? = null,
    output_mode: OutputMode = OutputMode.Native,
    tool_mode: ToolMode = ToolMode.Native,
    parallel_tool_calls: bool = true,
) -> Gemini throws never {
    Gemini {
        model: model,
        project_id: project_id,
        location: location,
        credentials: credentials,
        credentials_content: credentials_content,
        api_key: api_key,
        base_url: base_url,
        extra_headers: extra_headers,
        extra_query: extra_query,
        extra_body: extra_body,
        output_mode: output_mode,
        tool_mode: tool_mode,
        parallel_tool_calls: parallel_tool_calls,
    }
}
