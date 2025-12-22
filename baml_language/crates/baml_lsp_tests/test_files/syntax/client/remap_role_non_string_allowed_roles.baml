client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", 123, "assistant"]
    remap_roles {
      user "human"
    }
  }
}

// error: values in allowed_roles must be strings. Got: number
//   -->  client/remap_role_non_string_allowed_roles.baml:5
//    | 
//  4 |     model "gpt-4o"
//  5 |     allowed_roles ["user", 123, "assistant"]
//    |

//----
//- diagnostics
// Error: Expected config key, found Comma
//    ╭─[ 0:5:26 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                          ┬  
//    │                          ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found IntegerLiteral
//    ╭─[ 0:5:28 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                            ─┬─  
//    │                             ╰─── Expected config key, found IntegerLiteral
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Comma
//    ╭─[ 0:5:31 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                               ┬  
//    │                               ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//    ╭─[ 0:5:33 ]
//    │
//  5 │     allowed_roles ["user", 123, "assistant"]
//    │                                 ┬  
//    │                                 ╰── Expected config key, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unclosed string literal, found EOF
//     ╭─[ 0:17:7 ]
//     │
//  17 │ //    |
//     │       ┬  
//     │       ╰── Expected Unclosed string literal, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:17:7 ]
//     │
//  17 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:17:7 ]
//     │
//  17 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
