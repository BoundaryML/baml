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
// Error: connect_timeout_ms must be non-negative, got: -1000ms
//    ╭─[ client_http_config_negative_timeout.baml:6:7 ]
//    │
//  6 │       connect_timeout_ms -1000
//    │       ─────────┬────────  
//    │                ╰────────── connect_timeout_ms must be non-negative, got: -1000ms
//    │ 
//    │ Note: Error code: E0025
// ───╯
