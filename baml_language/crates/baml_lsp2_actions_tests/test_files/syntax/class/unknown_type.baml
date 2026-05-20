class InterfaceTwo {
  iface string
  prop2 Bar
}

// error: Type `Bar` does not exist. Did you mean one of these: `int`, `bool`, `float`, `true`, `false`, `string`, `InterfaceTwo`?
//   -->  class/unknown_type.baml:3
//    |
//  2 |   iface string
//  3 |   prop2 Bar
//    |

//----
//- diagnostics
// Error: unresolved type: Bar
//    ╭─[ class_unknown_type.baml:3:8 ]
//    │
//  3 │   prop2 Bar
//    │        ──┬─  
//    │          ╰─── unresolved type: Bar
//    │ 
//    │ Note: Error code: E0002
// ───╯
