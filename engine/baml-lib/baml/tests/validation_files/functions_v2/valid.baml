client<llm> Bar {
  provider baml-openai-chat
}

function Foo() -> int {
  client Bar
  prompt #"fa"#
}

function FooBar(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}