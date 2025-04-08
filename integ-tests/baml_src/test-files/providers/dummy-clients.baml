client OpenAIDummyClient {
  provider openai-generic
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4o-mini"
    base_url "http://localhost:8000"
  }
}

function TestOpenAIDummyClient(input: string) -> string {
  client OpenAIDummyClient
  prompt #"
    {{ _.role("user") }}
    {{ input }}
  "#
}