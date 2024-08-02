// Intentionally use a bad key
client<llm> BadClient {
  provider openai
  options {
    model "gpt-3.5-turbo"
    api_key "sk-invalid"
  }
}

function ExpectFailure() -> string {
  client BadClient

  prompt #"
    What is the capital of England?
  "#
}
