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
  prop String
  prop2 Int
  propArray String[]
  boolean Bool
}

class Type2 {
  interface1 Type1

  // comment
  prop2 String
}

function FunctionOne {
  input Type1
  // comment
  output Type2
}

function FunctionTwo {
  input Type1
  output String
}





generator Default {
  language python
  output "hello"
  switches {
    cheap true
    FooBar:cheap false
  }
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
  input String
  output String
}

impl<llm, FooBar2> SomeName {
  client MyClient
  prompt #"
    does something
    asdf #// This is a comment //# goes here
    a ;asdf;kljla
    asflk;jasdf
    {#input.some_field}

    {#print_type<json>(output)}
  "#
}


