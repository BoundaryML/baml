function Foo {
  input string
  output string
}

impl<llm, Foo> FooImpl{
  prompt "hello world"
}




// warning: To use comments and {#vars} use a block string. #"..."# instead.
//   -->  variants/require_client_field.baml:7
//    | 
//  6 | impl<llm, Foo> FooImpl{
//  7 |   prompt "hello world"
//    | 
// error: Error validating: Missing `client` field in impl<llm>
//   -->  variants/require_client_field.baml:6
//    | 
//  5 | 
//  6 | impl<llm, Foo> FooImpl{
//    | 
