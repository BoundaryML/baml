client<llm> MyClient {
  provider baml-openai-chat
  myExtraProp "hello"
  options {
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
//   -->  client/unknown_prop.baml:1
//    | 
//    | 
//  1 | client<llm> MyClient {
//  2 |   provider baml-openai-chat
//  3 |   myExtraProp "hello"
//  4 |   options {
//  5 |     max_tokens 100
//  6 |   }
//  7 | }
//    | 
