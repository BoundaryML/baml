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
    A: ${#input.a}
    B: ${#input.b}

    {#print_enum(AsStr, Bar)}
    {#print_type(JSON, output)}
  "#
}