function Foo(bla: int) -> int {
    client GPT4
    prompt #"Return 4"#
}


client<llm> GPT4 {
  provider openai
  options {
    model gpt-4o
    api_key env.OPENAI_API_KEY
  }
}

test FooTest {
  functions [Foo]
  args {}
}
