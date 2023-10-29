function Foo {
  input InputType
  output OutputType
}

class InputType {
  a string
  b bool
}

class OutputType {
  x string
  y bool
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

// error: Error validating: Unknown field `e` in class `InputType`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:23
//    | 
// 22 |   prompt #"
// 23 |     A {#input.e}
// 24 |     {#InputType.e}
//    | 
// error: Error validating: Must start with `input`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:20
//    | 
// 19 | 
// 20 | impl<llm, Foo> FooImpl{
// 21 |   client MyClient
// 22 |   prompt #"
// 23 |     A {#input.e}
// 24 |     {#InputType.e}
// 25 |     {#input.e.y}
// 26 |     
// 27 |     the rest of the prompt
// 28 |       tabbed-in
// 29 |     JSON:
// 30 |     {#print_enum(enumarg)}
// 31 |     {#print_type(typearg)}
// 32 |   "#
// 33 | }
//    | 
// error: Error validating: Unknown field `e` in class `InputType`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:25
//    | 
// 24 |     {#InputType.e}
// 25 |     {#input.e.y}
// 26 |     
//    | 
