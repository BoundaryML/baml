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
    A {#input.a}
    
    the rest of the prompt
      tabbed-in
    JSON:
    {#print_enum(enumarg)}
    {#print_type(typearg)}
  "#
}