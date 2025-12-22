client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "assistant", "system"]
    remap_roles {
      user "human"
      assistant "ai"
      system "instructions"
    }
    default_role "user"
  }
}

function TestRemap(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("user") }}
    Test message: {{ input }}
  "#
}

//----
//- diagnostics
// Error: Expected config key, found Comma
//    ╭─[ 0:5:26 ]
//    │
//  5 │     allowed_roles ["user", "assistant", "system"]
//    │                          ┬  
//    │                          ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//    ╭─[ 0:5:28 ]
//    │
//  5 │     allowed_roles ["user", "assistant", "system"]
//    │                            ┬  
//    │                            ╰── Expected config key, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:18:5 ]
//     │
//  18 │     {{ _.chat("user") }}
//     │     ┬  
//     │     ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:18:6 ]
//     │
//  18 │     {{ _.chat("user") }}
//     │      ┬  
//     │      ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:5 ]
//     │
//  19 │     Test message: {{ input }}
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:10 ]
//     │
//  19 │     Test message: {{ input }}
//     │          ───┬───  
//     │             ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Colon
//     ╭─[ 0:19:17 ]
//     │
//  19 │     Test message: {{ input }}
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Colon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:19:19 ]
//     │
//  19 │     Test message: {{ input }}
//     │                   ┬  
//     │                   ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:19:20 ]
//     │
//  19 │     Test message: {{ input }}
//     │                    ┬  
//     │                    ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:19:22 ]
//     │
//  19 │     Test message: {{ input }}
//     │                      ──┬──  
//     │                        ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:19:28 ]
//     │
//  19 │     Test message: {{ input }}
//     │                            ┬  
//     │                            ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:19:29 ]
//     │
//  19 │     Test message: {{ input }}
//     │                             ┬  
//     │                             ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:20:3 ]
//     │
//  20 │   "#
//     │   ┬  
//     │   ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Hash
//     ╭─[ 0:20:4 ]
//     │
//  20 │   "#
//     │    ┬  
//     │    ╰── Expected top-level declaration, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:21:1 ]
//     │
//  21 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
