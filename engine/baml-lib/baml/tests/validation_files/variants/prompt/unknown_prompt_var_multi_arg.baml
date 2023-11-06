class Bar {
  hello string
}

// TODO: finish this validation test..
function Foo {
  input (arg_name: Bar, arg_2: Bar)
  output string
}


client<llm> MyClient {
  provider baml-openai-chat
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


// error: Error validating: Must start with `input`
//   -->  /Users/vbv/repos/gloo-lang/engine/baml/baml/tests/validation_files/variants/prompt/unknown_prompt_var_multi_arg.baml:20
//    | 
// 19 |     A {#input.e}
// 20 |     {#InputType.e}
//    | 
