client<llm> OpenAIWithAnthropicResponse {
  provider azure-openai
  options {
    model "gpt-4o"
    client_response_type "anthropic"
    base_url "http://localhost:8000"
  }
}

function OpenAIWithAnthropicResponseHello(s: string) -> string {
  client OpenAIWithAnthropicResponse
  prompt #"
    Return the string "Hello, world!"
  "#
}
