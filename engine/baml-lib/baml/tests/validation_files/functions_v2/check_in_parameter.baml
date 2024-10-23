client<llm> MyClient {
  provider baml-openai-chat
}

class Foo {
  bar Bar?
  baz int
}

class Bar {
  name string @check(nonempty_name, {{ this|length > 0 }})
}

function Go(a: Foo) -> int {
  client MyClient
  prompt #"Hi"#
}

// error: Error validating: Types with checks are not allowed as function parameters.
//   -->  functions_v2/check_in_parameter.baml:14
//    | 
// 13 | 
// 14 | function Go(a: Foo) -> int {
//    | 
