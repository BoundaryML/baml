generator Default {
  language python
  output /Users/vbv/repos/gloo-lang/clients/python/baml_client/_generated
}

// Enum 
enum Sentiment {
  SAD
  HAPPY 
}

enum Sentiment2 {
  // comment
  ANGRY
}

// Interface blocks 
class Type1 {
  prop string
  prop2 int
  propArray string[]
  boolean bool
}

class Type2 {
  interface1 Type1

  // comment
  prop2 string
}

function FunctionOne {
  input Type1
  // comment
  output Type2
}

function FunctionTwo {
  input Type1
  output string
}




enum FAAAAA {
  POSITIVE
  NEGATIVE
  HEALTHY
}

// This is preferred
client<llm> MyClient {
  provider openai
  model gpt-35-turbo
  my_custom_var "some string with spaces"
 
}

//switch<client<llm>> MyClient {
//  default cheap
//  cheap SomeOtherClient
//  expensive SuperPowerClient
//  anthropic AnthropicClient
//}

function FooBar2 {
  input string
  output string
}

impl<llm, FooBar2> SomeName {
  client MyClient
  prompt #"
    does something
    asdf {// This is a comment //} goes here
    a ;asdf;kljla
    asflk;jasdf
    {#input}

    {#print_type(output)}
  "#
}
