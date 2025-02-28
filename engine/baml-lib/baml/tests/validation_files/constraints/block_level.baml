class Foo {
  foo int
  bar string
  @@check(foo_and_bar, {{ this.string|length < this.foo }})
}

class Foo2 {
  foo int
  bar string
  @@assert({{ this.string|length < this.foo }})
}

class Foo3 {
  foo int @check(foo_check, {{ this }})
  bar string @assert(hi, {{ this }}) @check(hi, {{ this }})
  @@assert({{ this.string|length < this.foo }})
  @@check(foo_check_2, {{ this }})
}

client<llm> GPT35 {
  provider baml-openai-chat
  options {
    model gpt-3.5-turbo
    max_tokens 100
  }
}

function FooFn(input: Foo2) -> Foo {
  client GPT35
  prompt #"
  "#
} 

test TestFoo {
  functions [FooFn]
  args {}
  @@check(foo_check_1, {{ this }})
  @@check(foo_check_2, {{ this }})
}