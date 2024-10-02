client<llm> Bar {
  provider baml-openai-chat
}

class Foo {
  age int @check({{this > 10}}, "old enough")
}

function FooToInt(foo: Foo, a: Foo @assert({{this.age > 20}}, "really old")) -> int @check({{ this < 10 }}) {
   client Bar
   prompt #"fa"#
}
