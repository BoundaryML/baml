client MyClient = openai.OpenAiClient.new(
  model = "gpt-4",
);

// Reliability composes with wrapper client values now (retry_policy blocks are gone).
client RetryClient = ai.clients.Retry.new(
  inner = anthropic.AnthropicClient.new(model = "claude-opus-4-1-20250805"),
  max_attempts = 3,
);

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// client.baml:1:1 (keyword) len=6 "client"
// client.baml:1:17 (operator) len=1 "="
// client.baml:1:19 (namespace) [defaultLibrary] len=6 "openai"
// client.baml:1:25 (namespace) [defaultLibrary] len=1 "."
// client.baml:1:26 (class) [defaultLibrary] len=12 "OpenAiClient"
// client.baml:1:39 (method) len=3 "new"
// client.baml:2:3 (parameter) len=5 "model"
// client.baml:2:9 (operator) len=1 "="
// client.baml:2:11 (string) len=7 "\"gpt-4\""
// client.baml:5:1 (comment) len=86 "// Reliability composes with wrapper client values now (retry_policy blocks are gone)."
// client.baml:6:1 (keyword) len=6 "client"
// client.baml:6:20 (operator) len=1 "="
// client.baml:6:22 (namespace) [defaultLibrary] len=2 "ai"
// client.baml:6:24 (namespace) [defaultLibrary] len=1 "."
// client.baml:6:25 (namespace) [defaultLibrary] len=7 "clients"
// client.baml:6:32 (namespace) [defaultLibrary] len=1 "."
// client.baml:6:33 (class) [defaultLibrary] len=5 "Retry"
// client.baml:6:39 (method) len=3 "new"
// client.baml:7:3 (parameter) len=5 "inner"
// client.baml:7:9 (operator) len=1 "="
// client.baml:7:11 (namespace) [defaultLibrary] len=9 "anthropic"
// client.baml:7:20 (namespace) [defaultLibrary] len=1 "."
// client.baml:7:21 (class) [defaultLibrary] len=15 "AnthropicClient"
// client.baml:7:37 (method) len=3 "new"
// client.baml:7:41 (parameter) len=5 "model"
// client.baml:7:47 (operator) len=1 "="
// client.baml:7:49 (string) len=26 "\"claude-opus-4-1-20250805\""
// client.baml:8:3 (parameter) len=12 "max_attempts"
// client.baml:8:16 (operator) len=1 "="
// client.baml:8:18 (number) len=1 "3"
