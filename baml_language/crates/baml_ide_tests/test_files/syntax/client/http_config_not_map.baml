client<llm> HttpNotMap {
  provider openai
  options {
    model "gpt-4"
    http "invalid"  // Should be a map
  }
}

// error: http must be a configuration block with timeout settings
//   -->  client/http_config_not_map.baml:5
//    | 
//  4 |     model "gpt-4"
//  5 |     http "invalid"  // Should be a map
//    |

//----
//- diagnostics
// Error: http must be a configuration block with timeout settings
//    ╭─[ client_http_config_not_map.baml:5:5 ]
//    │
//  5 │     http "invalid"  // Should be a map
//    │     ──┬─  
//    │       ╰─── http must be a configuration block with timeout settings
//    │ 
//    │ Note: Error code: E0023
// ───╯
