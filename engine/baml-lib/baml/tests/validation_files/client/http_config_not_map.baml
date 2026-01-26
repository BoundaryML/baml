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
