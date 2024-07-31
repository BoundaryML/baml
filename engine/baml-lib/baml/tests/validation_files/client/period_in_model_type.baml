client<llm> MyClient {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    max_tokens 100
  }
}

// error: Error validating function "MyClient": Invalid function syntax. Valid function syntax is
// ```
// function MyClient(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  client/period_in_model_type.baml:1
//    | 
//    | 
//  1 | client<llm> MyClient {
//  2 |   provider baml-openai-chat
//  3 |   options {
//  4 |     model gpt-3.5-turbo
//  5 |     max_tokens 100
//  6 |   }
//  7 | }
//    | 
