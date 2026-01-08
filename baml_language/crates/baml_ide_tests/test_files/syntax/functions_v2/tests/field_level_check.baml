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

//----
//- diagnostics
// Error: Expected config key, found '@'
//     ╭─[ functions_v2_tests_field_level_check.baml:12:5 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │     ┬  
//     │     ╰── Expected config key, found '@'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found '{'
//     ╭─[ functions_v2_tests_field_level_check.baml:12:24 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                        ┬  
//     │                        ╰── Expected config key, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found '{'
//     ╭─[ functions_v2_tests_field_level_check.baml:12:25 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                         ┬  
//     │                         ╰── Expected config key, found '{'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ functions_v2_tests_field_level_check.baml:12:33 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found ')'
//     ╭─[ functions_v2_tests_field_level_check.baml:12:34 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found ')'
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found '}'
//     ╭─[ functions_v2_tests_field_level_check.baml:13:1 ]
//     │
//  13 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found '}'
//     │ 
//     │ Note: Error code: E0010
// ────╯
