class InterfaceTwo {
  iface strin
}

class InterfaceOne {
  uses InterfaceT
}

// error: Type `strin` does not exist. Did you mean one of these: `string`, `true`, `int`, `float`, `bool`, `false`, `InterfaceOne`, `InterfaceTwo`?
//   -->  class/spelling_error.baml:2
//    |
//  1 | class InterfaceTwo {
//  2 |   iface strin
//    |
// error: Type `InterfaceT` does not exist. Did you mean one of these: `InterfaceTwo`, `InterfaceOne`, `int`, `true`, `string`, `float`, `false`, `bool`?
//   -->  class/spelling_error.baml:6
//    |
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    |

//----
//- diagnostics
// Error: unresolved type: strin
//    ╭─[ class_spelling_error.baml:2:8 ]
//    │
//  2 │   iface strin
//    │        ───┬──  
//    │           ╰──── unresolved type: strin
//    │ 
//    │ Note: Error code: E0002
// ───╯
// Error: unresolved type: InterfaceT
//    ╭─[ class_spelling_error.baml:6:7 ]
//    │
//  6 │   uses InterfaceT
//    │       ─────┬─────  
//    │            ╰─────── unresolved type: InterfaceT
//    │ 
//    │ Note: Error code: E0002
// ───╯
