function Foo {
  input InputType
  output OutputType
}

class InputType {
  a InputType2
  b Bool
}

class OtherInput {
  d String
}

class InputType2 {
  c String
}

class OutputType {
  x String
  y Bool
}

client<llm> MyClient {
  provider openai
}

enum Sentiment {
  HAPPY
}

impl<llm, Foo> FooImpl{
  client MyClient
  prompt #"
    A {#input.a}!! {// A
    multiline
    comment //}


    {#input.a.c}
    
    the rest of the prompt
    no-tab
      tab1
        tab2
    morespaces here
    {#InputType.a} {#input.a} hi there
    JSON:
    {#print_enum(Sentiment)}
    {#print_type(output)}
  "#
}