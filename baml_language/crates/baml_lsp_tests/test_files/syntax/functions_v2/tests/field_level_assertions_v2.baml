// Test that field-level assertions are not allowed on test block fields

function TestFunction(input: string) -> string {
  client "openai/gpt-4"
  prompt #"Test function"#
}

test MyTest {
  functions [TestFunction] @assert({{ true }})
  args {
    input "hello"
  }
}

// error: Error validating: @assert is not allowed on test fields. Use @@assert at the test block level instead.
//   -->  functions_v2/tests/field_level_assertions_v2.baml:9
//    | 
//  8 | test MyTest {
//  9 |   functions [TestFunction] @assert({{ true }})
//    |

//----
//- diagnostics
// Error: Expected config key, found At
//    ╭─[ 0:9:28 ]
//    │
//  9 │   functions [TestFunction] @assert({{ true }})
//    │                            ┬  
//    │                            ╰── Expected config key, found At
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found LBrace
//    ╭─[ 0:9:36 ]
//    │
//  9 │   functions [TestFunction] @assert({{ true }})
//    │                                    ┬  
//    │                                    ╰── Expected config key, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found LBrace
//    ╭─[ 0:9:37 ]
//    │
//  9 │   functions [TestFunction] @assert({{ true }})
//    │                                     ┬  
//    │                                     ╰── Expected config key, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:9:45 ]
//    │
//  9 │   functions [TestFunction] @assert({{ true }})
//    │                                             ┬  
//    │                                             ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RParen
//    ╭─[ 0:9:46 ]
//    │
//  9 │   functions [TestFunction] @assert({{ true }})
//    │                                              ┬  
//    │                                              ╰── Expected top-level declaration, found RParen
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:10:3 ]
//     │
//  10 │   args {
//     │   ──┬─  
//     │     ╰─── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found LBrace
//     ╭─[ 0:10:8 ]
//     │
//  10 │   args {
//     │        ┬  
//     │        ╰── Expected top-level declaration, found LBrace
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:5 ]
//     │
//  11 │     input "hello"
//     │     ──┬──  
//     │       ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:11:11 ]
//     │
//  11 │     input "hello"
//     │           ┬  
//     │           ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Word
//     ╭─[ 0:11:12 ]
//     │
//  11 │     input "hello"
//     │            ──┬──  
//     │              ╰──── Expected top-level declaration, found Word
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found Quote
//     ╭─[ 0:11:17 ]
//     │
//  11 │     input "hello"
//     │                 ┬  
//     │                 ╰── Expected top-level declaration, found Quote
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected top-level declaration, found RBrace
//     ╭─[ 0:12:3 ]
//     │
//  12 │   }
//     │   ┬  
//     │   ╰── Expected top-level declaration, found RBrace
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
