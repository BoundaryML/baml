// Anthropic client for version analysis
client<llm> ClaudeSonnet {
  provider anthropic
  retry_policy StandardRetry
  options {
    model "claude-sonnet-4-20250514"
    api_key env.ANTHROPIC_API_KEY
    max_tokens 4096
    temperature 0
  }
}

// Retry policy with exponential backoff
retry_policy StandardRetry {
  max_retries 3
  strategy {
    type exponential_backoff
    delay_ms 1000
    multiplier 2
    max_delay_ms 10000
  }
}
