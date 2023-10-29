function Foo {
  input string
  output string
}

impl<llm, Foo> FooImpl {
  prompt "hello world"
  client MadeUpClient
}

// error: Error validating: Unknown client `MadeUpClient`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/client_unknown.baml:6
//    | 
//  5 | 
//  6 | impl<llm, Foo> FooImpl {
//  7 |   prompt "hello world"
//  8 |   client MadeUpClient
//  9 | }
//    | 
