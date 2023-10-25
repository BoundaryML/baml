function Foo {
  input String
  output String
}

impl<llm, Foo> FooImpl{
  prompt "hello world"
}

// error: Error validating: Missing `client` field in variant<llm>
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt_client_required.baml:6
//    | 
//  5 | 
//  6 | impl<llm, Foo> FooImpl{
//  7 |   prompt "hello world"
//  8 | }
//    | 
