client<llm> Hello {
  provider baml-openai-chat // Trailing chat
  options {
    thing "word with \"quotes\""
  }
}

//----
//- diagnostics
// Error: Expected config key, found LBrace
//    ╭─[ 0:3:11 ]
//    │
//  3 │   options {
//    │           ┬  
//    │           ╰── Expected config key, found LBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected top-level declaration, found RBrace
//    ╭─[ 0:6:1 ]
//    │
//  6 │ }
//    │ ┬  
//    │ ╰── Expected top-level declaration, found RBrace
//    │ 
//    │ Note: Error code: E0010
// ───╯
