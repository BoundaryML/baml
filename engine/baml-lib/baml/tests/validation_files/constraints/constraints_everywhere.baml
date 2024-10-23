client<llm> Bar {
  provider baml-openai-chat
}

class Foo {
  age int @assert(old_enough, {{this > 10}})
}

function FooToInt(foo: Foo, a: Foo @assert(really_old, {{this.age > 20}})) -> int @check(small_int, {{ this < 10 }}) {
   client Bar
   prompt #"fa"#
}
