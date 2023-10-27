function Foo {
  input InputType
  output OutputType
}

class InputType {
  a String
  b Bool
}

class OutputType {
  x String
  y Bool
}

client<llm> MyClient {
  provider openai
}

impl<llm, Foo> FooImpl{
  client MyClient
  prompt #"
    A {#input.e}
    {#InputType.e}
    {#input.e.y}
    
    the rest of the prompt
      tabbed-in
    JSON:
    {#print_enum(enumarg)}
    {#print_type(typearg)}
  "#
}
// error: Error validating: Unknown attribute `e` in class `InputType`
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:23
//    | 
// 22 |   prompt #"
// 23 |     A {#input.e}
// 24 |     {#InputType.e}
//    | 
// error: Error validating: Unknown attribute `e` in class `InputType`
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:25
//    | 
// 24 |     {#InputType.e}
// 25 |     {#input.e.y}
// 26 |     
//    | 
