generator Default {
  language python
  output ../generate
}

// This is preferred
client<llm> SomeOtherClient {
  @@provider(openai)
  @@retry(3)
  @@default

  model gpt-3-test
  my_custom_var "some string with spaces"
  dictionary_key {
    array_key [ "abc", "abc", aaa    ]
    key2 "some random value"
    "key with" "is this value"
  }
}

class FooBar {
  package Int
  fighter String
}

function FooBar {
  input Int[] | String
  output (Int | string[])[]
}

variant<llm> TestMe {
  client {
    GPT4Client default
  }

  prompt #"
    What the fuck is going on here?!!
  "#
}
