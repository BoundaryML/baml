// The error here is bad.
function Foo {
  input string
  output string
}

client<llm> MyClient {
  provider baml-openai-chat
}

impl<llm, Foo> V1 {
  client<llm> MyClient
  prompt "Hello there"
}



// warning: To use comments and {#vars} use a block string. #"..."# instead.
//   -->  variants/bad_client_def.baml:13
//    | 
// 12 |   client<llm> MyClient
// 13 |   prompt "Hello there"
//    | 
// error: Error validating: Did you mean `client` instead of `client<...>`?
//   -->  variants/bad_client_def.baml:12
//    | 
// 11 | impl<llm, Foo> V1 {
// 12 |   client<llm> MyClient
//    | 
