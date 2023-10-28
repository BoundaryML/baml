// The error here is bad.
function Foo {
  input String
  output String
}
client<llm> MyClient {
  provider openai
}


impl<llm, Foo> V1 {
  client<llm> MyClient
  prompt "Hello there"
}
// error: Error validating: Unknown client `unquoted_string_literal(121, 135)`
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/bad_client_def.baml:10
//    | 
//  9 | 
// 10 | impl<llm, Foo> V1 {
// 11 |   client<llm> MyClient
// 12 |   prompt "Hello there"
// 13 | }
//    | 
