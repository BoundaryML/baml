// How Google AI Studio Gemini returns the task's typed value.
enum OutputMode {
    // Use controlled JSON generation and decode the exact JSON as T.
    Native,
    // Return text and recover T with BAML's schema-aware parser.
    Sap,
}

// How an agent exposes application tools to Google AI Studio Gemini.
enum ToolMode {
    // Use Gemini functionDeclarations and functionResponse content parts.
    Native,
    // Put T | ToolCalls in the prompt. Requires OutputMode.Sap.
    Prompt,
}

// Gemini Developer API adapter. The Google Cloud fields apply only when
// enterprise or GOOGLE_GENAI_USE_VERTEXAI routes this provider through Vertex.
class Gemini {
    model: string,
    api_key: string?,
    enterprise: bool?,
    project_id: string?,
    location: string?,
    credentials: string?,
    credentials_content: string?,
    base_url: string?,
    extra_headers: map<string, string>?,
    extra_query: map<string, string>?,
    extra_body: map<string, unknown>?,
    output_mode: OutputMode,
    tool_mode: ToolMode,
    parallel_tool_calls: bool,
}

implements ai.Provider for Gemini {
  function name(self) -> string throws never { "google-ai" }
  function render_shorthand(self) -> string throws never {
    "google-ai/" + self.model
  }
}

function gemini(
    model: string = "gemini-2.5-flash",
    api_key: string? = null,
    enterprise: bool? = null,
    project_id: string? = null,
    location: string? = null,
    credentials: string? = null,
    credentials_content: string? = null,
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
        api_key: api_key,
        enterprise: enterprise,
        project_id: project_id,
        location: location,
        credentials: credentials,
        credentials_content: credentials_content,
        base_url: base_url,
        extra_headers: extra_headers,
        extra_query: extra_query,
        extra_body: extra_body,
        output_mode: output_mode,
        tool_mode: tool_mode,
        parallel_tool_calls: parallel_tool_calls,
    }
}
