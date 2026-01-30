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

// warning: Test 'FooTest' is missing required arguments for function 'Foo'. Add an args block like:
// 
// args {
//   bla 123
// }
//   -->  tests/missing_arg.baml:15
//    | 
// 14 | 
// 15 | test FooTest {
// 16 |   functions [Foo]
// 17 |   args {}
// 18 | }
//    | 
