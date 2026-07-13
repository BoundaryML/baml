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

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// retry_policy_valid_retry.baml:1:1 (comment) len=72 "// Minimal repro: retry_policy inside client block causes parsing errors"
// retry_policy_valid_retry.baml:3:1 (keyword) len=6 "client"
// retry_policy_valid_retry.baml:3:7 (operator) len=1 "<"
// retry_policy_valid_retry.baml:3:8 (type) len=3 "llm"
// retry_policy_valid_retry.baml:3:11 (operator) len=1 ">"
// retry_policy_valid_retry.baml:3:13 (struct) [declaration] len=8 "MyClient"
// retry_policy_valid_retry.baml:4:3 (property) len=8 "provider"
// retry_policy_valid_retry.baml:5:3 (property) len=12 "retry_policy"
// retry_policy_valid_retry.baml:6:3 (property) len=7 "options"
// retry_policy_valid_retry.baml:7:5 (property) len=5 "model"
// retry_policy_valid_retry.baml:7:11 (string) len=7 "\"gpt-4\""
// retry_policy_valid_retry.baml:11:1 (keyword) len=12 "retry_policy"
// retry_policy_valid_retry.baml:11:14 (struct) [declaration] len=7 "MyRetry"
// retry_policy_valid_retry.baml:12:3 (property) len=11 "max_retries"
// retry_policy_valid_retry.baml:12:15 (number) len=1 "3"
