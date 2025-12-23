template_string WithParams(a: int) #"
  ...
"#

template_string BadCall1 #"
  {{ WithParams(a=2, b=2) }}
"#

template_string BadCall2 #"
  {{ WithParams("a") }}
"#

template_string BadCall3 #"
  {{ WithParams() }}
"#

template_string BadCall4 #"
  {{ Random(2) }}
"#

// warning: Function 'WithParams' expects 1 arguments, but got 2
//   -->  template_string/bad_calls.baml:6
//    | 
//  5 | template_string BadCall1 #"
//  6 |   {{ WithParams(a=2, b=2) }}
//    | 
// warning: Function 'WithParams' expects argument 'a' to be of type int, but got literal["a"]
//   -->  template_string/bad_calls.baml:10
//    | 
//  9 | template_string BadCall2 #"
// 10 |   {{ WithParams("a") }}
//    | 
// warning: Function 'WithParams' expects 1 arguments, but got 0
//   -->  template_string/bad_calls.baml:14
//    | 
// 13 | template_string BadCall3 #"
// 14 |   {{ WithParams() }}
//    | 
// warning: Variable `Random` does not exist. Did you mean one of these: `_`, `ctx`?
//   -->  template_string/bad_calls.baml:18
//    | 
// 17 | template_string BadCall4 #"
// 18 |   {{ Random(2) }}
//    | 
// warning: 'Random' is undefined, expected function
//   -->  template_string/bad_calls.baml:18
//    | 
// 17 | template_string BadCall4 #"
// 18 |   {{ Random(2) }}
//    | 
// warning: Function 'WithParams' expects 1 arguments, but got 2
//   -->  template_string/bad_calls.baml:6
//    | 
//  5 | template_string BadCall1 #"
//  6 |   {{ WithParams(a=2, b=2) }}
//    | 
// warning: Function 'WithParams' expects argument 'a' to be of type int, but got literal["a"]
//   -->  template_string/bad_calls.baml:10
//    | 
//  9 | template_string BadCall2 #"
// 10 |   {{ WithParams("a") }}
//    | 
// warning: Function 'WithParams' expects 1 arguments, but got 0
//   -->  template_string/bad_calls.baml:14
//    | 
// 13 | template_string BadCall3 #"
// 14 |   {{ WithParams() }}
//    | 
// warning: Variable `Random` does not exist. Did you mean one of these: `_`, `ctx`?
//   -->  template_string/bad_calls.baml:18
//    | 
// 17 | template_string BadCall4 #"
// 18 |   {{ Random(2) }}
//    | 
// warning: 'Random' is undefined, expected function
//   -->  template_string/bad_calls.baml:18
//    | 
// 17 | template_string BadCall4 #"
// 18 |   {{ Random(2) }}
//    |

//----
//- diagnostics
// Error: Expected LParen, found Hash
//    ╭─[ 0:5:26 ]
//    │
//  5 │ template_string BadCall1 #"
//    │                          ┬  
//    │                          ╰── Expected LParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found Hash
//    ╭─[ 0:5:26 ]
//    │
//  5 │ template_string BadCall1 #"
//    │                          ┬  
//    │                          ╰── Expected parameter name, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found Hash
//    ╭─[ 0:5:26 ]
//    │
//  5 │ template_string BadCall1 #"
//    │                          ┬  
//    │                          ╰── Expected type annotation, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found Hash
//    ╭─[ 0:5:26 ]
//    │
//  5 │ template_string BadCall1 #"
//    │                          ┬  
//    │                          ╰── Expected RParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected LParen, found Hash
//    ╭─[ 0:9:26 ]
//    │
//  9 │ template_string BadCall2 #"
//    │                          ┬  
//    │                          ╰── Expected LParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected parameter name, found Hash
//    ╭─[ 0:9:26 ]
//    │
//  9 │ template_string BadCall2 #"
//    │                          ┬  
//    │                          ╰── Expected parameter name, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected type annotation, found Hash
//    ╭─[ 0:9:26 ]
//    │
//  9 │ template_string BadCall2 #"
//    │                          ┬  
//    │                          ╰── Expected type annotation, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected RParen, found Hash
//    ╭─[ 0:9:26 ]
//    │
//  9 │ template_string BadCall2 #"
//    │                          ┬  
//    │                          ╰── Expected RParen, found Hash
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected LParen, found Hash
//     ╭─[ 0:13:26 ]
//     │
//  13 │ template_string BadCall3 #"
//     │                          ┬  
//     │                          ╰── Expected LParen, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected parameter name, found Hash
//     ╭─[ 0:13:26 ]
//     │
//  13 │ template_string BadCall3 #"
//     │                          ┬  
//     │                          ╰── Expected parameter name, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type annotation, found Hash
//     ╭─[ 0:13:26 ]
//     │
//  13 │ template_string BadCall3 #"
//     │                          ┬  
//     │                          ╰── Expected type annotation, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Hash
//     ╭─[ 0:13:26 ]
//     │
//  13 │ template_string BadCall3 #"
//     │                          ┬  
//     │                          ╰── Expected RParen, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected LParen, found Hash
//     ╭─[ 0:17:26 ]
//     │
//  17 │ template_string BadCall4 #"
//     │                          ┬  
//     │                          ╰── Expected LParen, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected parameter name, found Hash
//     ╭─[ 0:17:26 ]
//     │
//  17 │ template_string BadCall4 #"
//     │                          ┬  
//     │                          ╰── Expected parameter name, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected type annotation, found Hash
//     ╭─[ 0:17:26 ]
//     │
//  17 │ template_string BadCall4 #"
//     │                          ┬  
//     │                          ╰── Expected type annotation, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
// Error: Expected RParen, found Hash
//     ╭─[ 0:17:26 ]
//     │
//  17 │ template_string BadCall4 #"
//     │                          ┬  
//     │                          ╰── Expected RParen, found Hash
//     │ 
//     │ Note: Error code: E0010
// ────╯
