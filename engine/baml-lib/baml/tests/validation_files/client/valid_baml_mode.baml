// Test that valid baml_mode values are accepted

client<llm> TextModeClient {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "text_schema"
  }
}

client<llm> ToolCallingModeClient {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "tool_calling"
  }
}

// Default mode (no baml_mode specified) should also work
client<llm> DefaultModeClient {
  provider openai
  options {
    model "gpt-4o"
  }
}

// Tool calling mode with tools configuration
client<llm> ToolCallingWithTools {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "tool_calling"
    tools [
      {
        type "function"
        function {
          name "get_weather"
          description "Get weather information"
          parameters {
            type "object"
            properties {
              location { type "string" }
            }
            required ["location"]
          }
        }
      }
    ]
    tool_choice "auto"
  }
}