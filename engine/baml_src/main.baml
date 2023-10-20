generator Default {
  language python
  output ../generate
  switches {
    cheap true
    FooBar:cheap false
  }
}

// This is preferred
client<llm> SomeOtherClient {
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

function FooBar {
  input Int[] | String
  output (Int | string[])[]
}

impl<llm, FooBar> SomeName {
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
    asdf {#// This is a comment //#} goes here
    a ;asdf;kljla
    asflk;jasdf
    {#input.some_field}

    {#print_type<json>(output)}
  "#
}


generator Default {
  language python
  output ../path/to/dir
  features {
    openai true
    anthropic true
    fast true
    slow true
  }
}

class Bar {
  a int
  b string
}
