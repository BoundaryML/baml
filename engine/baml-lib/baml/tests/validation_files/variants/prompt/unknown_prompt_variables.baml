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
  provider baml-openai-chat
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

enum Enumarg {
  A
  B
  C
}










// error: Error validating: Unknown field `e` in class `InputType`
//   -->  variants/prompt/unknown_prompt_variables.baml:23
//    | 
// 22 |   prompt #"
// 23 |     A {#input.e}
//    | 
// error: Error validating: Unknown field `e` in class `InputType`
//   -->  variants/prompt/unknown_prompt_variables.baml:24
//    | 
// 23 |     A {#input.e}
// 24 |     {#input.e}
//    | 
// error: Error validating: Unknown field `e` in class `InputType`
//   -->  variants/prompt/unknown_prompt_variables.baml:25
//    | 
// 24 |     {#input.e}
// 25 |     {#input.e.y}
//    | 
// error: Enum `enumarg` does not exist. No Enums are used in the output of this function.
//   -->  variants/prompt/unknown_prompt_variables.baml:30
//    | 
// 29 |     JSON:
// 30 |     {#print_enum(enumarg)}
//    | 
// error: Type `typearg` does not exist. Did you mean one of these: `output`, `OutputType`?
//   -->  variants/prompt/unknown_prompt_variables.baml:31
//    | 
// 30 |     {#print_enum(enumarg)}
// 31 |     {#print_type(typearg)}
//    | 
