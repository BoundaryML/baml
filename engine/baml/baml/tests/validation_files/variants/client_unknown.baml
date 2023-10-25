function Foo {
  input String
  output String
}



impl<llm, Foo> FooImpl {
  prompt "hello world"
  client MadeUpClient
}

// error: Error validating: Unknown client `MadeUpClient`
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt_client_unknown.baml:8
//    | 
//  7 | 
//  8 | impl<llm, Foo> FooImpl {
//  9 |   prompt "hello world"
// 10 |   client MadeUpClient
// 11 | }
//    | 
