function Foo {
  input InputType
  output OutputType
  default_impl FooImpl
}

class InputType {
  a InputType2
  b bool
}

class OtherInput {
  d string
}

class InputType2 {
  c string
}

class OutputType {
  x Sentiment
  y bool
}

client<llm> MyClient {
  provider baml-openai-chat
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
    {#input.a} {#input.a} hi there
    JSON:
    {#print_enum(Sentiment)}
    {#print_type(output)}
  "#
}



impl<llm, Foo> FooImpl3 {
  // Include a trailing space
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
    {#input.a} {#input.a} hi there
    JSON:
    {#print_enum(Sentiment)}
    {#print_type(output)}
  "#
}



