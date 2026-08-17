// retry_policy blocks are gone; retries wrap the client value instead.
client MyClient = ai.clients.Retry.new(
  inner = openai.ResponsesClient.new(model = "gpt-4"),
  max_attempts = 3,
);

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// retry_policy_valid_retry.baml:1:1 (comment) len=71 "// retry_policy blocks are gone; retries wrap the client value instead."
// retry_policy_valid_retry.baml:2:1 (keyword) len=6 "client"
// retry_policy_valid_retry.baml:2:17 (operator) len=1 "="
// retry_policy_valid_retry.baml:2:19 (namespace) [defaultLibrary] len=2 "ai"
// retry_policy_valid_retry.baml:2:21 (namespace) [defaultLibrary] len=1 "."
// retry_policy_valid_retry.baml:2:22 (namespace) [defaultLibrary] len=7 "clients"
// retry_policy_valid_retry.baml:2:29 (namespace) [defaultLibrary] len=1 "."
// retry_policy_valid_retry.baml:2:30 (class) [defaultLibrary] len=5 "Retry"
// retry_policy_valid_retry.baml:2:36 (method) len=3 "new"
// retry_policy_valid_retry.baml:3:3 (parameter) len=5 "inner"
// retry_policy_valid_retry.baml:3:9 (operator) len=1 "="
// retry_policy_valid_retry.baml:3:11 (namespace) [defaultLibrary] len=6 "openai"
// retry_policy_valid_retry.baml:3:17 (namespace) [defaultLibrary] len=1 "."
// retry_policy_valid_retry.baml:3:18 (class) [defaultLibrary] len=15 "ResponsesClient"
// retry_policy_valid_retry.baml:3:34 (method) len=3 "new"
// retry_policy_valid_retry.baml:3:38 (parameter) len=5 "model"
// retry_policy_valid_retry.baml:3:44 (operator) len=1 "="
// retry_policy_valid_retry.baml:3:46 (string) len=7 "\"gpt-4\""
// retry_policy_valid_retry.baml:4:3 (parameter) len=12 "max_attempts"
// retry_policy_valid_retry.baml:4:16 (operator) len=1 "="
// retry_policy_valid_retry.baml:4:18 (number) len=1 "3"
