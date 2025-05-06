client<llm> OpenAIWithFinishReasonError {
  provider openai
  options {
    api_key env.OPENAI_API_KEY
    model "gpt-4"
    max_tokens 10
    finish_reason_allow_list ["stop"]
  }
}

function TestOpenAIWithFinishReasonError(input: string) -> string {
  client OpenAIWithFinishReasonError
  prompt #"
    {{ _.role("user") }} Write a haiku.
  "#
}