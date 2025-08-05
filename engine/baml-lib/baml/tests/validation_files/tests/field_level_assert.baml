function SimpleFunction(input: string) -> string {
  client "openai/gpt-3.5-turbo"
  prompt #"Return hello {{input}}"#
}

test TestFieldLevelAssert {
  functions [SimpleFunction]
  args {
    input "test"
  }
  @assert(this == "Hello, test!")  // This should error
}

// error: Error parsing attribute "assert": Test assertions must use block-level syntax '@@assert' instead of '@assert'. Example:
//   test MyTest {
//     functions [MyFunc]
//     args {}
//     @@assert(label, {{ this == "expected" }})
//   }
//   -->  tests/field_level_assert.baml:11
//    | 
// 10 |   }
// 11 |   @assert(this == "Hello, test!")  // This should error
//    | 
