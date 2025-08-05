// Test that field-level @check is not allowed on test block fields

function TestFunction(input: string) -> string {
  client "openai/gpt-4"
  prompt #"Test function"#
}

test MyTest {
  functions [TestFunction]
  args {
    input "hello"
  } @check(args_check, {{ true }})
}

// error: Error validating: @check is not allowed on test fields. Use @@check at the test block level instead.
//   -->  functions_v2/tests/field_level_check.baml:12
//    | 
// 11 |     input "hello"
// 12 |   } @check(args_check, {{ true }})
//    | 
