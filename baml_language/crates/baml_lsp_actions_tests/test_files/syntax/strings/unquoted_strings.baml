client<llm> Hello {
  provider baml-openai-chat
  options {
    thing hello'world
    banned @helloworld
    banned2 #helloworld
    banned3 hello(world)
  }
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  strings/unquoted_strings.baml:1
//    |
//    |
//  1 | client<llm> Hello {
//  2 |   provider baml-openai-chat
//    |

//----
//- diagnostics
// Error: Expected config key, found error
//    ╭─[ strings_unquoted_strings.baml:4:16 ]
//    │
//  4 │     thing hello'world
//    │                ┬  
//    │                ╰── Expected config key, found error
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected identifier, found '#'
//    ╭─[ strings_unquoted_strings.baml:6:13 ]
//    │
//  6 │     banned2 #helloworld
//    │             ┬  
//    │             ╰── Expected identifier, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found '#'
//    ╭─[ strings_unquoted_strings.baml:6:13 ]
//    │
//  6 │     banned2 #helloworld
//    │             ┬  
//    │             ╰── Expected config key, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected identifier, found '('
//    ╭─[ strings_unquoted_strings.baml:7:18 ]
//    │
//  7 │     banned3 hello(world)
//    │                  ┬  
//    │                  ╰── Expected identifier, found '('
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found '('
//    ╭─[ strings_unquoted_strings.baml:7:18 ]
//    │
//  7 │     banned3 hello(world)
//    │                  ┬  
//    │                  ╰── Expected config key, found '('
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected identifier, found ')'
//    ╭─[ strings_unquoted_strings.baml:7:24 ]
//    │
//  7 │     banned3 hello(world)
//    │                        ┬  
//    │                        ╰── Expected identifier, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected config key, found ')'
//    ╭─[ strings_unquoted_strings.baml:7:24 ]
//    │
//  7 │     banned3 hello(world)
//    │                        ┬  
//    │                        ╰── Expected config key, found ')'
//    │ 
//    │ Note: Error code: E0010
// ───╯
