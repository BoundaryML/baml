// Valid HTTP configuration for regular clients
client<llm> ValidHttpClient {
  provider openai
  options {
    model "gpt-4"
    http {
      connect_timeout_ms 5000
      request_timeout_ms 30000
    }
  }
}

// Valid streaming timeouts
client<llm> ValidStreamingClient {
  provider anthropic
  options {
    model "claude-3"
    http {
      time_to_first_token_timeout_ms 15000
      idle_timeout_ms 10000
    }
  }
}

// Valid zero timeout (infinite)
client<llm> InfiniteTimeoutClient {
  provider openai
  options {
    model "gpt-4"
    http {
      request_timeout_ms 0
    }
  }
}

// Valid composite client with total timeout
client<llm> ValidFallbackClient {
  provider fallback
  options {
    strategy [ValidHttpClient, ValidStreamingClient]
    http {
      total_timeout_ms 60000
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// http_config.baml:1:1 (comment) len=47 "// Valid HTTP configuration for regular clients"
// http_config.baml:2:1 (keyword) len=6 "client"
// http_config.baml:2:7 (operator) len=1 "<"
// http_config.baml:2:8 (type) len=3 "llm"
// http_config.baml:2:11 (operator) len=1 ">"
// http_config.baml:2:13 (struct) [declaration] len=15 "ValidHttpClient"
// http_config.baml:3:3 (property) len=8 "provider"
// http_config.baml:4:3 (property) len=7 "options"
// http_config.baml:5:5 (property) len=5 "model"
// http_config.baml:5:11 (string) len=7 "\"gpt-4\""
// http_config.baml:6:5 (property) len=4 "http"
// http_config.baml:7:7 (property) len=18 "connect_timeout_ms"
// http_config.baml:7:26 (number) len=4 "5000"
// http_config.baml:8:7 (property) len=18 "request_timeout_ms"
// http_config.baml:8:26 (number) len=5 "30000"
// http_config.baml:13:1 (comment) len=27 "// Valid streaming timeouts"
// http_config.baml:14:1 (keyword) len=6 "client"
// http_config.baml:14:7 (operator) len=1 "<"
// http_config.baml:14:8 (type) len=3 "llm"
// http_config.baml:14:11 (operator) len=1 ">"
// http_config.baml:14:13 (struct) [declaration] len=20 "ValidStreamingClient"
// http_config.baml:15:3 (property) len=8 "provider"
// http_config.baml:16:3 (property) len=7 "options"
// http_config.baml:17:5 (property) len=5 "model"
// http_config.baml:17:11 (string) len=10 "\"claude-3\""
// http_config.baml:18:5 (property) len=4 "http"
// http_config.baml:19:7 (property) len=30 "time_to_first_token_timeout_ms"
// http_config.baml:19:38 (number) len=5 "15000"
// http_config.baml:20:7 (property) len=15 "idle_timeout_ms"
// http_config.baml:20:23 (number) len=5 "10000"
// http_config.baml:25:1 (comment) len=32 "// Valid zero timeout (infinite)"
// http_config.baml:26:1 (keyword) len=6 "client"
// http_config.baml:26:7 (operator) len=1 "<"
// http_config.baml:26:8 (type) len=3 "llm"
// http_config.baml:26:11 (operator) len=1 ">"
// http_config.baml:26:13 (struct) [declaration] len=21 "InfiniteTimeoutClient"
// http_config.baml:27:3 (property) len=8 "provider"
// http_config.baml:28:3 (property) len=7 "options"
// http_config.baml:29:5 (property) len=5 "model"
// http_config.baml:29:11 (string) len=7 "\"gpt-4\""
// http_config.baml:30:5 (property) len=4 "http"
// http_config.baml:31:7 (property) len=18 "request_timeout_ms"
// http_config.baml:31:26 (number) len=1 "0"
// http_config.baml:36:1 (comment) len=44 "// Valid composite client with total timeout"
// http_config.baml:37:1 (keyword) len=6 "client"
// http_config.baml:37:7 (operator) len=1 "<"
// http_config.baml:37:8 (type) len=3 "llm"
// http_config.baml:37:11 (operator) len=1 ">"
// http_config.baml:37:13 (struct) [declaration] len=19 "ValidFallbackClient"
// http_config.baml:38:3 (property) len=8 "provider"
// http_config.baml:39:3 (property) len=7 "options"
// http_config.baml:40:5 (property) len=8 "strategy"
// http_config.baml:41:5 (property) len=4 "http"
// http_config.baml:42:7 (property) len=16 "total_timeout_ms"
// http_config.baml:42:24 (number) len=5 "60000"
