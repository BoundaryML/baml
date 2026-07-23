// OpenAI Chat Completions wire models used by the tool-loop capability.

class OpenAiToolMessage {
  content: string?,
  tool_calls: json[]?,
}

class OpenAiToolChoice {
  message: OpenAiToolMessage,
}

class OpenAiToolResponse {
  choices: OpenAiToolChoice[],
}

class OpenAiToolTranscript {
  model: string,
  api_key: string,
  base_url: string?,
  tools: json[],
  messages: json[],
  response_format: map<string, json>?,
}
