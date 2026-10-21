client<llm> AwsPooling {
  provider aws-bedrock
  options {
    model "anthropic.claude-3-sonnet-20240229-v1:0"
    http {
      enable_connection_pooling true
    }
  }
}

// error: enable_connection_pooling is not supported for aws clients because their HTTP transport is managed externally
//   -->  client/http_config_aws_pooling.baml:5
//    | 
//  4 |     model "anthropic.claude-3-sonnet-20240229-v1:0"
//  5 |     http {
//  6 |       enable_connection_pooling true
//  7 |     }
//    | 
