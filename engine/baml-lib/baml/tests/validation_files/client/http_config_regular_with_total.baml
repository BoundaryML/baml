client<llm> RegularWithTotal {
  provider openai
  options {
    model "gpt-4"
    http {
      total_timeout_ms 60000  // Only for composite clients
    }
  }
}

// error: Unrecognized field 'total_timeout_ms' in http configuration block. 'total_timeout_ms' is only available for composite clients (fallback/round-robin). For regular clients, use: connect_timeout_ms, request_timeout_ms, time_to_first_token_timeout_ms, idle_timeout_ms
//   -->  client/http_config_regular_with_total.baml:5
//    | 
//  4 |     model "gpt-4"
//  5 |     http {
//  6 |       total_timeout_ms 60000  // Only for composite clients
//  7 |     }
//    | 
