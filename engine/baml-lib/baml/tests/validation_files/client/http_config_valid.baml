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