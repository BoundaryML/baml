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
// Warning: Function 'WithParams' expects argument 'arg0'
//     ╭─[ template_string_bad_calls.baml:14:5 ]
//     │
//  14 │   {{ WithParams() }}
//     │     ──────┬─────  
//     │           ╰─────── Function 'WithParams' expects argument 'arg0'
//     │ 
//     │ Note: Error code: E0070
// ────╯
// Warning: Variable `Random` does not exist. Did you mean one of these: `_`, `ctx`, `BadCall1`?
//     ╭─[ template_string_bad_calls.baml:18:5 ]
//     │
//  18 │   {{ Random(2) }}
//     │     ───┬──  
//     │        ╰──── Variable `Random` does not exist. Did you mean one of these: `_`, `ctx`, `BadCall1`?
//     │ 
//     │ Note: Error code: E0070
// ────╯
// Warning: Function 'WithParams' does not have an argument 'a'. Did you mean 'arg0'?
//    ╭─[ template_string_bad_calls.baml:6:5 ]
//    │
//  6 │   {{ WithParams(a=2, b=2) }}
//    │     ──────────┬─────────  
//    │               ╰─────────── Function 'WithParams' does not have an argument 'a'. Did you mean 'arg0'?
//    │ 
//    │ Note: Error code: E0070
// ───╯
// Warning: Function 'WithParams' does not have an argument 'b'. Did you mean 'arg0'?
//    ╭─[ template_string_bad_calls.baml:6:5 ]
//    │
//  6 │   {{ WithParams(a=2, b=2) }}
//    │     ──────────┬─────────  
//    │               ╰─────────── Function 'WithParams' does not have an argument 'b'. Did you mean 'arg0'?
//    │ 
//    │ Note: Error code: E0070
// ───╯
// Warning: Function 'WithParams' expects argument 'arg0'
//    ╭─[ template_string_bad_calls.baml:6:5 ]
//    │
//  6 │   {{ WithParams(a=2, b=2) }}
//    │     ──────────┬─────────  
//    │               ╰─────────── Function 'WithParams' expects argument 'arg0'
//    │ 
//    │ Note: Error code: E0070
// ───╯
