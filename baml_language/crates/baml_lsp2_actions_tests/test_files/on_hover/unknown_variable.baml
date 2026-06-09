function UseUnknown() -> int {
  unknown_var + 1
}

//----
//- diagnostics
// error: unresolved name: unknown_var
//    ╭─[ unknown_variable.baml:2:3 ]
//    │
//  2 │   unknown_var + 1
//    │   ─────┬─────  
//    │        ╰─────── unresolved name: unknown_var
//    │ 
//    │ Note: Error code: E0003
// ───╯
