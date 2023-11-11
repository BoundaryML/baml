function Foo {
  input string
  output string
}

impl<llm, Foo> FooImpl{
  prompt "hello world"
}



// error: Error validating: Missing `client` field in variant<llm>
//   -->  variants/require_client_field.baml:6
//    | 
//  5 | 
//  6 | impl<llm, Foo> FooImpl{
//    | 
