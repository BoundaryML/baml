function Foo {
  input string
  output string
}

client<llm> MyClient {
  provider baml-openai-chat
}

impl<llm> Impl1 {
  prompt "hello"
  client MyClient
}

impl Impl2 {
  prompt "hello"
  client MyClient
}


// error: Error validating: impl requires 2 template args. (did you forget <llm, FunctionName>)
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/missing_template_args.baml:10
//    | 
//  9 | 
// 10 | impl<llm> Impl1 {
// 11 |   prompt "hello"
// 12 |   client MyClient
// 13 | }
//    | 
// error: Error validating: Missing template for impl. (did you forget <llm, FunctionName>)
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/missing_template_args.baml:15
//    | 
// 14 | 
// 15 | impl Impl2 {
// 16 |   prompt "hello"
// 17 |   client MyClient
// 18 | }
//    | 
