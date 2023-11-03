function Foo {
  input string
  output string
}

impl<llm, Foo> FooImpl{
  prompt "hello world"
}



// error: Error validating: Missing `client` field in variant<llm>
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/require_client_field.baml:6
//    | 
//  5 | 
//  6 | impl<llm, Foo> FooImpl{
//  7 |   prompt "hello world"
//  8 | }
//    | 
