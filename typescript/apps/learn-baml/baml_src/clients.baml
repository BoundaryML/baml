client<llm> ClaudeSonnet {
  provider anthropic
  retry_policy StandardRetry
  options {
    model "claude-sonnet-4-5-20250929"
    api_key env.ANTHROPIC_API_KEY
    max_tokens 2048
  }
}

retry_policy StandardRetry {
  max_retries 2
  strategy {
    type exponential_backoff
    delay_ms 500
    multiplier 2
    max_delay_ms 5000
  }
}
