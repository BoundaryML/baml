client<llm> MyClient {
  options {
    max_tokens 100
  }
}





// error: Error validating: Missing `provider` field in client. e.g. `provider baml-openai-chat`
//   -->  client/required_provider.baml:1
//    | 
//    | 
//  1 | client<llm> MyClient {
//    | 
