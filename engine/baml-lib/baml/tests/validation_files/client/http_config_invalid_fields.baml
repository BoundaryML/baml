client<llm> InvalidFieldName {
  provider openai
  options {
    model "gpt-4"
    http {
      conect_timeout_ms 5000  // Typo in field name
    }
  }
}

// error: Unrecognized field 'conect_timeout_ms' in http configuration block. Did you mean 'connect_timeout_ms'?
//   -->  client/http_config_invalid_fields.baml:5
//    | 
//  4 |     model "gpt-4"
//  5 |     http {
//  6 |       conect_timeout_ms 5000  // Typo in field name
//  7 |     }
//    | 
