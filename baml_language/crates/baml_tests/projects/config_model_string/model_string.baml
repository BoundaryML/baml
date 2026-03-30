client<llm> MyAwsClient {
  provider aws-bedrock
  options {
    model "anthropic.claude-3-haiku-20240307-v1:0"
    max_tokens 100
    temperature 0.7
    api_key "sk-1234"
  }
}
