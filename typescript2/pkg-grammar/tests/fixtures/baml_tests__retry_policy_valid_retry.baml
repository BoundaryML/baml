// Minimal repro: retry_policy inside client block causes parsing errors

client<llm> MyClient {
  provider openai
  retry_policy MyRetry
  options {
    model "gpt-4"
  }
}

retry_policy MyRetry {
  max_retries 3
}
