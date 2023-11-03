function Foo {
  input string
  output string
}

impl<llm, Foo> FooImpl {
  client MadeUpClient
  prompt #"
  hello world {#input}
  {#print_type(output)}
  "#
}



// error: Error validating: Unknown client `MadeUpClient`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/client_unknown.baml:7
//    | 
//  6 | impl<llm, Foo> FooImpl {
//  7 |   client MadeUpClient
//    | 
