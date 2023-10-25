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
class Interface1 {
  prop String
  prop2 Int
  propArray String[]
  boolean Bool
}

class InterfaceTwo {
  interface1 Interface1

  // comment
  prop2 String
}

function FunctionOne {
  input Interface1
  // comment
  output InterfaceTwo
}

function FunctionTwo {
  input Interface1
  output String
}





generator Default {
  language python
  output /Users/aaronvillalpando/Projects/gloo-lang/clients/python/baml_client/generated
  switches {
    cheap true
    FooBar:cheap false
  }
}

// generator Default {
//   language python
//   output ../path/to/dir
//   features {
//     openai true
//     anthropic true
//     fast true
//     slow true
//   }
// }

enum FAAAAA {
  POSITIVE
  NEGATIVE
  HEALTHY
}

// This is preferred
client<llm> MyClient {
  @@provider(openai)
  @@retry(3)

  model gpt-3-test
  my_custom_var "some string with spaces"
  dictionary_key {
    array_key [ "abc", "abc", aaa    ]
    key2 "some random value"
    "key with" "is this value"
  }
}

//switch<client<llm>> MyClient {
//  default cheap
//  cheap SomeOtherClient
//  expensive SuperPowerClient
//  anthropic AnthropicClient
//}

function FooBar2 {
  input Int[] | String
  output (Int | String[])[]
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

impl<llm, FooBar> SomeName2 {
  client MyClient
  prompt #"
    does something
    asdf {#// This is a comment //#} {#//goes//#} here
    {#// This is awesome}
    a ;asdf;kljla
    asflk;jasdf\t
    {#input.some_field}

    {#print_type<json>(output)}
  "#
}


class Bar {
  a Int
  b String
}
