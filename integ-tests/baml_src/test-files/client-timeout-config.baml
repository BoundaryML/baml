// Client with extremely short timeouts for testing
client<llm> TestTimeoutClient {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
    http {
      connect_timeout_ms 1  // 1ms - will timeout on connect
      request_timeout_ms 10  // 10ms - very likely to timeout
    }
  }
}

// Function that will timeout
function TestTimeoutError(input: string) -> string {
  client TestTimeoutClient
  prompt #"
    This is a test that should timeout.
    Please write a very long response about: {{input}}
  "#
}

// Client with only request timeout
client<llm> TestRequestTimeoutClient {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 50  // 50ms timeout
    }
  }
}

function TestRequestTimeout(input: string) -> string {
  client TestRequestTimeoutClient
  prompt #"
    Generate a detailed 500 word essay about: {{input}}
  "#
}

// Client with zero timeout (infinite)
client<llm> TestZeroTimeoutClient {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
    http {
      request_timeout_ms 0  // 0 means infinite
    }
  }
}

function TestZeroTimeout(input: string) -> string {
  client TestZeroTimeoutClient
  prompt #"
    Echo: {{input}}
  "#
}

// Fallback client that includes timeout client
client<llm> TestTimeoutFallbackClient {
  provider fallback
  options {
    strategy [TestTimeoutClient, TestZeroTimeoutClient]
  }
}

function TestTimeoutFallback(input: string) -> string {
  client TestTimeoutFallbackClient
  prompt #"
    Simple echo: {{input}}
  "#
}

// Client for streaming timeout tests (Phase 4)
client<llm> TestStreamingTimeoutClient {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
    http {
      time_to_first_token_timeout_ms 1  // 1ms - extremely short, should always timeout
      idle_timeout_ms 1  // 1ms - extremely short, should always timeout
    }
  }
}

function TestStreamingTimeout(input: string) -> string {
  client TestStreamingTimeoutClient
  prompt #"
    Stream a long response about: {{input}}
  "#
}
