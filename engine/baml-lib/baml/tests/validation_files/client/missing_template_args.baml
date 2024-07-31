client MyClient {
  provider baml-openai-chat
}

// error: Error validating function "MyClient": Invalid function syntax. Valid function syntax is
// ```
// function MyClient(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  client/missing_template_args.baml:1
//    | 
//    | 
//  1 | client MyClient {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
