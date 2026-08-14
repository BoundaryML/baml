client<llm> NegativeTimeout {
  provider openai
  options {
    model "gpt-4"
    http {
      connect_timeout_ms -1000
    }
  }
}

// error: connect_timeout_ms must be non-negative, got: -1000ms
//   -->  client/http_config_negative_timeout.baml:6
//    |
//  5 |     http {
//  6 |       connect_timeout_ms -1000
//    |

//----
//- diagnostics
// E0010
//
//   × `client<llm>` config blocks are removed; declare a client value instead: `client NegativeTimeout = openai.OpenAiClient.new(model = "...");` (compose reliability with ai.Retry / ai.Fallback /
//   │ ai.RoundRobin)
//    ╭─[client_http_config_negative_timeout.baml:1:1]
//  1 │ ╭─▶ client<llm> NegativeTimeout {
//  2 │ │     provider openai
//  3 │ │     options {
//  4 │ │       model "gpt-4"
//  5 │ │       http {
//  6 │ │         connect_timeout_ms -1000
//  7 │ │       }
//  8 │ │     }
//  9 │ ├─▶ }
//    · ╰──── replace with `client Name = <expr>;`
//    ╰────
