class Bar {
  hello string
}

// TODO: finish this validation test..
function Foo {
  input (arg_name: Bar, arg_2: Bar)
  output string
}


client<llm> MyClient {
  provider openai
}

impl<llm, Foo> FooImpl{
  client MyClient
  prompt #"
    A {#input.e}
    {#InputType.e}
    
    the rest of the prompt
      tabbed-in
    JSON:
    {#print_enum(enumarg)}
    {#print_type(typearg)}
  "#
}

// error: Error validating: Unknown arg `e`. Could be one of: arg_name, arg_2
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_var_multi_arg.baml:16
//    | 
// 15 | 
// 16 | impl<llm, Foo> FooImpl{
// 17 |   client MyClient
// 18 |   prompt #"
// 19 |     A {#input.e}
// 20 |     {#InputType.e}
// 21 |     
// 22 |     the rest of the prompt
// 23 |       tabbed-in
// 24 |     JSON:
// 25 |     {#print_enum(enumarg)}
// 26 |     {#print_type(typearg)}
// 27 |   "#
// 28 | }
//    | 
// error: Error validating: Must start with `input`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_var_multi_arg.baml:16
//    | 
// 15 | 
// 16 | impl<llm, Foo> FooImpl{
// 17 |   client MyClient
// 18 |   prompt #"
// 19 |     A {#input.e}
// 20 |     {#InputType.e}
// 21 |     
// 22 |     the rest of the prompt
// 23 |       tabbed-in
// 24 |     JSON:
// 25 |     {#print_enum(enumarg)}
// 26 |     {#print_type(typearg)}
// 27 |   "#
// 28 | }
//    | 
