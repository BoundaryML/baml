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
// Error: Expected '(', found '#'
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected '(', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found '#'
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected parameter name, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found '#'
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected type annotation, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found '#'
//    ╭─[ 0:5:27 ]
//    │
//  5 │ template_string GoodCall1 #"
//    │                           ┬  
//    │                           ╰── Expected ')', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected '(', found '#'
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected '(', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found '#'
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected parameter name, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found '#'
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected type annotation, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found '#'
//    ╭─[ 0:9:27 ]
//    │
//  9 │ template_string GoodCall2 #"
//    │                           ┬  
//    │                           ╰── Expected ')', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
