template_string SomeString #"
  ...
"#

template_string WithParams(a: int) #"
  ...
"#

//----
//- diagnostics
// Error: Expected '(', found '#'
//    ╭─[ template_string_valid.baml:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected '(', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found '#'
//    ╭─[ template_string_valid.baml:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected parameter name, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found '#'
//    ╭─[ template_string_valid.baml:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected type annotation, found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected ')', found '#'
//    ╭─[ template_string_valid.baml:1:28 ]
//    │
//  1 │ template_string SomeString #"
//    │                            ┬  
//    │                            ╰── Expected ')', found '#'
//    │ 
//    │ Note: Error code: E0010
// ───╯
