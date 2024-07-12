// An example of a BAML client with a retry policy

// https://docs.boundaryml.com/docs/snippets/clients/retry


retry_policy MyPolicyName {
  max_retries 3
}

client<llm> MyClient {
  provider anthropic
  retry_policy MyPolicyName
  options {
    model "claude-3-sonnet-20240229"
    api_key env.ANTHROPIC_API_KEY
  }
}