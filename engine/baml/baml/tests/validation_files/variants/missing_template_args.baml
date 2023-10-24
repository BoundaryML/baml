function Foo {
  input String
  output String
}

client<llm> MyClient {
  provider openai
}

impl<llm> {
  prompt "hello"
  client MyClient
}

impl {
  prompt "hello"
  client MyClient
}

//
// error
//