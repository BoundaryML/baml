// Constructed client values (the old client<llm> http config blocks are gone)
client ValidHttpClient = openai.ResponsesClient.new(
  model = "gpt-4",
  base_url = "http://localhost:8080",
);

client ValidStreamingClient = anthropic.AnthropicClient.new(model = "claude-3");

client InfiniteTimeoutClient = openai.ResponsesClient.new(model = "gpt-4");

// Composite reliability client built from the two above
client ValidFallbackClient = ai.clients.Fallback { members: [ValidHttpClient, ValidStreamingClient] };

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// http_config.baml:1:1 (comment) len=78 "// Constructed client values (the old client<llm> http config blocks are gone)"
// http_config.baml:2:1 (keyword) len=6 "client"
// http_config.baml:2:24 (operator) len=1 "="
// http_config.baml:2:26 (namespace) [defaultLibrary] len=6 "openai"
// http_config.baml:2:32 (namespace) [defaultLibrary] len=1 "."
// http_config.baml:2:33 (class) [defaultLibrary] len=15 "ResponsesClient"
// http_config.baml:2:49 (method) len=3 "new"
// http_config.baml:3:3 (parameter) len=5 "model"
// http_config.baml:3:9 (operator) len=1 "="
// http_config.baml:3:11 (string) len=7 "\"gpt-4\""
// http_config.baml:4:3 (parameter) len=8 "base_url"
// http_config.baml:4:12 (operator) len=1 "="
// http_config.baml:4:14 (string) len=23 "\"http://localhost:8080\""
// http_config.baml:7:1 (keyword) len=6 "client"
// http_config.baml:7:29 (operator) len=1 "="
// http_config.baml:7:31 (namespace) [defaultLibrary] len=9 "anthropic"
// http_config.baml:7:40 (namespace) [defaultLibrary] len=1 "."
// http_config.baml:7:41 (class) [defaultLibrary] len=15 "AnthropicClient"
// http_config.baml:7:57 (method) len=3 "new"
// http_config.baml:7:61 (parameter) len=5 "model"
// http_config.baml:7:67 (operator) len=1 "="
// http_config.baml:7:69 (string) len=10 "\"claude-3\""
// http_config.baml:9:1 (keyword) len=6 "client"
// http_config.baml:9:30 (operator) len=1 "="
// http_config.baml:9:32 (namespace) [defaultLibrary] len=6 "openai"
// http_config.baml:9:38 (namespace) [defaultLibrary] len=1 "."
// http_config.baml:9:39 (class) [defaultLibrary] len=15 "ResponsesClient"
// http_config.baml:9:55 (method) len=3 "new"
// http_config.baml:9:59 (parameter) len=5 "model"
// http_config.baml:9:65 (operator) len=1 "="
// http_config.baml:9:67 (string) len=7 "\"gpt-4\""
// http_config.baml:11:1 (comment) len=56 "// Composite reliability client built from the two above"
// http_config.baml:12:1 (keyword) len=6 "client"
// http_config.baml:12:28 (operator) len=1 "="
// http_config.baml:12:30 (class) len=2 "ai"
// http_config.baml:12:32 (namespace) [defaultLibrary] len=1 "."
// http_config.baml:12:40 (namespace) [defaultLibrary] len=1 "."
// http_config.baml:12:52 (property) len=7 "members"
// http_config.baml:12:62 (variable) len=15 "ValidHttpClient"
// http_config.baml:12:79 (variable) len=20 "ValidStreamingClient"
