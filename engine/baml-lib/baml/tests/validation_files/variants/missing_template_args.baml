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
//   -->  variants/missing_template_args.baml:10
//    | 
//  9 | 
// 10 | impl<llm> Impl1 {
//    | 
// error: Error validating: Missing template for impl. (did you forget <llm, FunctionName>)
//   -->  variants/missing_template_args.baml:15
//    | 
// 14 | 
// 15 | impl Impl2 {
//    | 
