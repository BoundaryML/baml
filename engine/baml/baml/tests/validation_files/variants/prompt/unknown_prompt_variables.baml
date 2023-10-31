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
    {#input.e}
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
//    | 
// error: Error validating: Unknown field `e` in class `InputType`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:24
//    | 
// 23 |     A {#input.e}
// 24 |     {#input.e}
//    | 
// error: Error validating: Unknown field `e` in class `InputType`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:25
//    | 
// 24 |     {#input.e}
// 25 |     {#input.e.y}
//    | 
// error: Error validating: Unknown enum `enumarg`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:30
//    | 
// 29 |     JSON:
// 30 |     {#print_enum(enumarg)}
//    | 
// error: Error validating: Unknown enum `typearg`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_variables.baml:31
//    | 
// 30 |     {#print_enum(enumarg)}
// 31 |     {#print_type(typearg)}
//    | 
