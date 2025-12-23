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
// Error: Expected client name, found Quote
//    ╭─[ 0:4:10 ]
//    │
//  4 │   client "openai/gpt-4"
//    │          ┬  
//    │          ╰── Expected client name, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:4:10 ]
//    │
//  4 │   client "openai/gpt-4"
//    │          ┬  
//    │          ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    ╭─[ 0:4:11 ]
//    │
//  4 │   client "openai/gpt-4"
//    │           ───┬──  
//    │              ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'openai', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    ╭─[ 0:4:17 ]
//    │
//  4 │   client "openai/gpt-4"
//    │                 ┬  
//    │                 ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '/', found Slash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4', found Word
//    ╭─[ 0:4:18 ]
//    │
//  4 │   client "openai/gpt-4"
//    │                  ──┬──  
//    │                    ╰──── Expected Only 'client' and 'prompt' allowed in LLM function, found 'gpt-4', found Word
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    ╭─[ 0:4:23 ]
//    │
//  4 │   client "openai/gpt-4"
//    │                       ┬  
//    │                       ╰── Expected Only 'client' and 'prompt' allowed in LLM function, found '"', found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found At
//     ╭─[ 0:12:5 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │     ┬  
//     │     ╰── Expected config key, found At
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found Comma
//     ╭─[ 0:12:22 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                      ┬  
//     │                      ╰── Expected config key, found Comma
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:12:24 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                        ┬  
//     │                        ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:12:25 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                         ┬  
//     │                         ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:12:33 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                                 ┬  
//     │                                 ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RParen
//     ╭─[ 0:12:34 ]
//     │
//  12 │   } @check(args_check, {{ true }})
//     │                                  ┬  
//     │                                  ╰── Expected top-level declaration, found RParen
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:13:1 ]
//     │
//  13 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
