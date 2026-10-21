client<llm> InvalidPoolingType {
  provider openai
  options {
    model "gpt-4"
    http {
      enable_connection_pooling "yes"
    }
  }
}

// error: enable_connection_pooling must be a bool. Got: string
//   -->  client/http_config_invalid_pooling.baml:6
//    | 
//  5 |     http {
//  6 |       enable_connection_pooling "yes"
//    | 
