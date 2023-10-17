generator Default {
  language python
  output ../generate
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

function FooBar {
  input Int[] | String
  output (Int | string[])[]
}

impl<llm, FooBar> SomeName {
  client SomeOtherClient
  prompt #"
    does something
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