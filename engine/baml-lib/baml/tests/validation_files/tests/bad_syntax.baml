class Input {
  str_field string
}

function Foo(input: Input[]) -> string {
  client "openai/gpt-4o-mini"
  prompt #"Tell me a haiku."#
}

test Foo {
  functions [Foo]
  input [
    {
      str_field "hello"
    },,
  ]
}

// warning: Direct values are not supported. Please pass in parameters by name
//   -->  tests/bad_syntax.baml:12
//    | 
// 11 |   functions [Foo]
// 12 |   input [
// 13 |     {
// 14 |       str_field "hello"
// 15 |     },,
// 16 |   ]
//    | 
// error: Error validating: Invalid array syntax detected.
//   -->  tests/bad_syntax.baml:15
//    | 
// 14 |       str_field "hello"
// 15 |     },,
// 16 |   ]
//    | 
