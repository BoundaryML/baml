function Foo {
  input InputType
  output OutputType
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
    {#chat()}
    the rest of the prompt
    no-tab
      tab1
        tab2
    morespaces here
    {#chat}
    {#input.a} {#input.a} hi there
    JSON:
    {#print_enum(Sentiment)}
    {#print_type(output)}
  "#
}



// error: Unexpected token. Expected one of: Expected exactly one argument for role. e.g. {#chat(user)} or {#chat(system)}
//   -->  variants/prompt/invalid_chat.baml:41
//    | 
// 40 |     {#input.a.c}
// 41 |     {#chat()}
//    | 
// error: Unexpected token. Expected one of: Expected exactly one argument for role. e.g. {#chat(user)} or {#chat(system)}
//   -->  variants/prompt/invalid_chat.baml:47
//    | 
// 46 |     morespaces here
// 47 |     {#chat}
//    | 
