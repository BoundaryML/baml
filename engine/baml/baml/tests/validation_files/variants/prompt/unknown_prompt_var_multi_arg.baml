class Bar {
  hello String
}

// TODO: finish this validation test..
function Foo {
  input (arg_name: Bar, arg_2: Bar)
  output String
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
// error: Error validating: Unknown attribute `e` in class `Bar`
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_var_multi_arg:18
//    | 
// 17 |   prompt #"
// 18 |     A {#input.e}
// 19 |     {#InputType.e}
//    | 
