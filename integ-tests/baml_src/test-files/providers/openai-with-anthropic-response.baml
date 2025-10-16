client<llm> OpenAIWithAnthropicResponse {
  provider openai-responses
  options {
    model "gpt-4o"
    client_response_type "openai-responses"
    base_url "http://localhost:8000"
  }
}

function OpenAIWithAnthropicResponseHello(s: string) -> string {
  client OpenAIWithAnthropicResponse
  prompt #"
    Return the string "Hello, world!" with {{ s }} included in the response.
    {{ _.role("user") }}
  "#
}

test TestOpenAIWithAnthropicResponse {
  functions [
    OpenAIWithAnthropicResponseHello
  ]
  args {
    s "Cherry blossoms"
  }
}