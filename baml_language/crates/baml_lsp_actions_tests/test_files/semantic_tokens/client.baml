client<llm> MyClient {
  provider openai
  options {
    model "gpt-4"
    max_tokens 100
  }
}

retry_policy MyRetry {
  max_retries 3
}

client<llm> RetryClient {
  provider anthropic
  retry_policy MyRetry
  options {
    model "claude-opus-4-1-20250805"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// client.baml:1:1 (keyword) len=6 "client"
// client.baml:1:7 (operator) len=1 "<"
// client.baml:1:8 (type) len=3 "llm"
// client.baml:1:11 (operator) len=1 ">"
// client.baml:1:13 (struct) len=8 "MyClient"
// client.baml:2:3 (property) len=8 "provider"
// client.baml:3:3 (property) len=7 "options"
// client.baml:4:5 (property) len=5 "model"
// client.baml:4:11 (string) len=1 "\""
// client.baml:4:12 (string) len=5 "gpt-4"
// client.baml:4:17 (string) len=1 "\""
// client.baml:5:5 (property) len=10 "max_tokens"
// client.baml:5:16 (number) len=3 "100"
// client.baml:9:1 (keyword) len=12 "retry_policy"
// client.baml:9:14 (struct) len=7 "MyRetry"
// client.baml:10:3 (property) len=11 "max_retries"
// client.baml:10:15 (number) len=1 "3"
// client.baml:13:1 (keyword) len=6 "client"
// client.baml:13:7 (operator) len=1 "<"
// client.baml:13:8 (type) len=3 "llm"
// client.baml:13:11 (operator) len=1 ">"
// client.baml:13:13 (struct) len=11 "RetryClient"
// client.baml:14:3 (property) len=8 "provider"
// client.baml:15:3 (property) len=12 "retry_policy"
// client.baml:16:3 (property) len=7 "options"
// client.baml:17:5 (property) len=5 "model"
// client.baml:17:11 (string) len=1 "\""
// client.baml:17:12 (string) len=24 "claude-opus-4-1-20250805"
// client.baml:17:36 (string) len=1 "\""
