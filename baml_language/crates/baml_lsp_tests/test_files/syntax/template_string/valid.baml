template_string SomeString #"
  ...
"#

template_string WithParams(a: int) #"
  ...
"#

//----
//- diagnostics
// Error: Expected '(', found '#'
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected '(', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found '#'
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected parameter name, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found '#'
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected type annotation, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found '#'
//    ╭─[ 0:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected ')', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
