client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant"]
    remap_roles {
      user "human"
      system "instructions"
    }
  }
}

// error: remap_roles values must be one of: Value("user"), Value("assistant"). Got: system. To support different remap roles, add allowed_roles ["user", "assistant", "system", ...]
//   -->  client/remap_role_invalid_role.baml:8
//    | 
//  7 |       user "human"
//  8 |       system "instructions"
//    |

//----
//- diagnostics
// Error: Expected config key, found Comma
//    ╭─[ 0:5:26 ]
//    │
//  5 │     allowed_roles ["user", "assistant"]
//    │                          ┬  
//    │                          ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//    ╭─[ 0:5:28 ]
//    │
//  5 │     allowed_roles ["user", "assistant"]
//    │                            ┬  
//    │                            ╰── Expected config key, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unclosed string literal, found EOF
//     ╭─[ 0:18:7 ]
//     │
//  18 │ //    |
//     │       ┬  
//     │       ╰── Expected Unclosed string literal, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:18:7 ]
//     │
//  18 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RBrace, found EOF
//     ╭─[ 0:18:7 ]
//     │
//  18 │ //    |
//     │       ┬  
//     │       ╰── Expected RBrace, found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
