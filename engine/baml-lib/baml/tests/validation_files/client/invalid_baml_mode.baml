// Test that invalid baml_mode values are rejected

client<llm> InvalidModeClient1 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "function_calling"  // Invalid value
  }
}

client<llm> InvalidModeClient2 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "tools"  // Invalid value
  }
}

client<llm> InvalidModeClient3 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode "json"  // Invalid value
  }
}

client<llm> InvalidModeClient4 {
  provider openai
  options {
    model "gpt-4o"
    baml_mode ""  // Empty string
  }
}

// error: Invalid baml_mode: function_calling. Must be 'text_schema' or 'tool_calling'
//   -->  client/invalid_baml_mode.baml:7
//    | 
//  6 |     model "gpt-4o"
//  7 |     baml_mode "function_calling"  // Invalid value
//    | 
// error: Invalid baml_mode: tools. Must be 'text_schema' or 'tool_calling'
//   -->  client/invalid_baml_mode.baml:15
//    | 
// 14 |     model "gpt-4o"
// 15 |     baml_mode "tools"  // Invalid value
//    | 
// error: Invalid baml_mode: json. Must be 'text_schema' or 'tool_calling'
//   -->  client/invalid_baml_mode.baml:23
//    | 
// 22 |     model "gpt-4o"
// 23 |     baml_mode "json"  // Invalid value
//    | 
// error: Invalid baml_mode: . Must be 'text_schema' or 'tool_calling'
//   -->  client/invalid_baml_mode.baml:31
//    | 
// 30 |     model "gpt-4o"
// 31 |     baml_mode ""  // Empty string
//    | 
