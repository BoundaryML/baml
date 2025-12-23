template_string WithParams(a: int) #"
  ...
"#

template_string GoodCall1 #"
  {{ WithParams(a=2) }}
"#

template_string GoodCall2 #"
  {{ WithParams(2) }}
"#

//----
//- diagnostics
// Error: Expected LParen, found Hash
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected LParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found Hash
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected parameter name, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found Hash
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected type annotation, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found Hash
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected RParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected LParen, found Hash
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected LParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found Hash
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected parameter name, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found Hash
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected type annotation, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found Hash
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected RParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
