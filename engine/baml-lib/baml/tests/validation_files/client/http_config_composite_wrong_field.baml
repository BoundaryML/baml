client<llm> BaseClient {
  provider openai
  options {
    model "gpt-4"
  }
}

client<llm> InvalidCompositeFields {
  provider fallback
  options {
    strategy [BaseClient]
    http {
      request_timeout_ms 5000  // Not allowed for composite
      connect_timeout_ms 3000  // Not allowed for composite
    }
  }
}

// error: Unrecognized field 'request_timeout_ms' in http configuration block. Did you mean 'total_timeout_ms'? Composite clients (fallback/round-robin) only support: total_timeout_ms
//   -->  client/http_config_composite_wrong_field.baml:12
//    | 
// 11 |     strategy [BaseClient]
// 12 |     http {
// 13 |       request_timeout_ms 5000  // Not allowed for composite
// 14 |       connect_timeout_ms 3000  // Not allowed for composite
// 15 |     }
//    | 
// error: Unrecognized field 'connect_timeout_ms' in http configuration block. Did you mean 'total_timeout_ms'? Composite clients (fallback/round-robin) only support: total_timeout_ms
//   -->  client/http_config_composite_wrong_field.baml:12
//    | 
// 11 |     strategy [BaseClient]
// 12 |     http {
// 13 |       request_timeout_ms 5000  // Not allowed for composite
// 14 |       connect_timeout_ms 3000  // Not allowed for composite
// 15 |     }
//    | 
