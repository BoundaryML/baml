client<llm> Bar {
  provider baml-openai-chat
}

class Foo {
  age int @assert({{this > 10}}, old_enough)
}

function FooToInt(foo: Foo, a: Foo @assert({{this.age > 20}}, really_old)) -> int @check({{ this < 10 }}, small_int) {
   client Bar
   prompt #"fa"#
}
