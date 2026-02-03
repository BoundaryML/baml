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
// Warning: Function 'WithParams' does not have an argument 'a'. Did you mean 'arg0'?
//    ╭─[ template_string_good_calls.baml:6:5 ]
//    │
//  6 │   {{ WithParams(a=2) }}
//    │     ───────┬───────  
//    │            ╰───────── Function 'WithParams' does not have an argument 'a'. Did you mean 'arg0'?
//    │ 
//    │ Note: Error code: E0070
// ───╯
// Warning: Function 'WithParams' expects argument 'arg0'
//    ╭─[ template_string_good_calls.baml:6:5 ]
//    │
//  6 │   {{ WithParams(a=2) }}
//    │     ───────┬───────  
//    │            ╰───────── Function 'WithParams' expects argument 'arg0'
//    │ 
//    │ Note: Error code: E0070
// ───╯
