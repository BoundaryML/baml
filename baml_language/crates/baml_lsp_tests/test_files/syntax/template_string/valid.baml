template_string SomeString #"
  ...
"#

template_string WithParams(a: int) #"
  ...
"#

//----
//- diagnostics
// Error: Expected LParen, found Hash
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected LParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found Hash
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected parameter name, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found Hash
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected type annotation, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found Hash
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected RParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
