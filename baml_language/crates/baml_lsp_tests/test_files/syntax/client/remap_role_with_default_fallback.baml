client<llm> MyClient {
  provider openai
  options {
    model "gpt-4o"
    allowed_roles ["user", "system"]
    default_role "system"
    remap_roles {
      system "instructions"
    }
  }
}

function TestRemapWithFallback(input: string) -> string {
  client MyClient
  prompt #"
    {{ _.chat("assistant") }}
    This should fallback to system role and then be remapped to instructions: {{ input }}
  "#
}

//----
//- diagnostics
// Error: Expected config key, found Comma
//    ╭─[ 0:5:26 ]
//    │
//  5 │     allowed_roles ["user", "system"]
//    │                          ┬  
//    │                          ╰── Expected config key, found Comma
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found Quote
//    ╭─[ 0:5:28 ]
//    │
//  5 │     allowed_roles ["user", "system"]
//    │                            ┬  
//    │                            ╰── Expected config key, found Quote
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:16:5 ]
//     │
//  16 │     {{ _.chat("assistant") }}
//     │     ┬  
//     │     ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected config key, found LBrace
//     ╭─[ 0:16:6 ]
//     │
//  16 │     {{ _.chat("assistant") }}
//     │      ┬  
//     │      ╰── Expected config key, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:5 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │     ──┬─  
//     │       ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:10 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │          ───┬──  
//     │             ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:17 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                 ────┬───  
//     │                     ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:26 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                          ─┬  
//     │                           ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:29 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                             ───┬──  
//     │                                ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:36 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                    ──┬─  
//     │                                      ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:41 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                         ─┬─  
//     │                                          ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:45 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                             ──┬─  
//     │                                               ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:50 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                  ─┬  
//     │                                                   ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:53 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                     ────┬───  
//     │                                                         ╰───── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:62 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                              ─┬  
//     │                                                               ╰── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:65 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                 ──────┬─────  
//     │                                                                       ╰─────── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Colon
//     ╭─[ 0:17:77 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                             ┬  
//     │                                                                             ╰── Expected top-level declaration, found Colon
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:17:79 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                               ┬  
//     │                                                                               ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:17:80 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                                ┬  
//     │                                                                                ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:17:82 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                                  ──┬──  
//     │                                                                                    ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:17:88 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                                        ┬  
//     │                                                                                        ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:17:89 ]
//     │
//  17 │     This should fallback to system role and then be remapped to instructions: {{ input }}
//     │                                                                                         ┬  
//     │                                                                                         ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:18:3 ]
//     │
//  18 │   "#
//     │   ┬  
//     │   ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Hash
//     ╭─[ 0:18:4 ]
//     │
//  18 │   "#
//     │    ┬  
//     │    ╰── Expected top-level declaration, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:19:1 ]
//     │
//  19 │ }
//     │ ┬  
//     │ ╰── Expected top-level declaration, found RBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
