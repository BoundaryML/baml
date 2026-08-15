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
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ValidHttpClient = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//     ╭─[client_http_config_valid.baml:2:1]
//   2 │ ╭─▶ client<llm> ValidHttpClient {
//   3 │ │     provider openai
//   4 │ │     options {
//   5 │ │       model "gpt-4"
//   6 │ │       http {
//   7 │ │         connect_timeout_ms 5000
//   8 │ │         request_timeout_ms 30000
//   9 │ │       }
//  10 │ │     }
//  11 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ValidStreamingClient = anthropic.AnthropicClient.new(model = "...");` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_http_config_valid.baml:14:1]
//  14 │ ╭─▶ client<llm> ValidStreamingClient {
//  15 │ │     provider anthropic
//  16 │ │     options {
//  17 │ │       model "claude-3"
//  18 │ │       http {
//  19 │ │         time_to_first_token_timeout_ms 15000
//  20 │ │         idle_timeout_ms 10000
//  21 │ │       }
//  22 │ │     }
//  23 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client InfiniteTimeoutClient = openai.ResponsesClient.new(model = "...");` (compose reliability with ai.Retry /
//   │ ai.Fallback / ai.RoundRobin)
//     ╭─[client_http_config_valid.baml:26:1]
//  26 │ ╭─▶ client<llm> InfiniteTimeoutClient {
//  27 │ │     provider openai
//  28 │ │     options {
//  29 │ │       model "gpt-4"
//  30 │ │       http {
//  31 │ │         request_timeout_ms 0
//  32 │ │       }
//  33 │ │     }
//  34 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client ValidFallbackClient = ai.Fallback.new(members = [FirstClient, SecondClient]);` (compose reliability with
//   │ ai.Retry / ai.Fallback / ai.RoundRobin)
//     ╭─[client_http_config_valid.baml:37:1]
//  37 │ ╭─▶ client<llm> ValidFallbackClient {
//  38 │ │     provider fallback
//  39 │ │     options {
//  40 │ │       strategy [ValidHttpClient, ValidStreamingClient]
//  41 │ │       http {
//  42 │ │         total_timeout_ms 60000
//  43 │ │       }
//  44 │ │     }
//  45 │ ├─▶ }
//     · ╰──── replace with `client Name = <expr>;`
//     ╰────
