function UseUnknown() -> int {
  unknown_var + 1
}

//----
//- diagnostics
// Error: Unknown variable `unknown_var`
//    ╭─[ unknown_variable.baml:2:3 ]
//    │
//  2 │   unknown_var + 1
//    │   ─────┬─────  
//    │        ╰─────── Unknown variable `unknown_var`
//    │ 
//    │ Note: Error code: E0003
// ───╯
