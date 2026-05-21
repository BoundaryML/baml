class InterfaceTwo {
  interface strin
}

class InterfaceOne {
  uses InterfaceT
}

// error: Type `strin` does not exist. Did you mean one of these: `string`, `true`, `int`, `float`, `bool`, `false`, `InterfaceOne`, `InterfaceTwo`?
//   -->  class/spelling_error.baml:2
//    |
//  1 | class InterfaceTwo {
//  2 |   interface strin
//    |
// error: Type `InterfaceT` does not exist. Did you mean one of these: `InterfaceTwo`, `InterfaceOne`, `int`, `true`, `string`, `float`, `false`, `bool`?
//   -->  class/spelling_error.baml:6
//    |
//  5 | class InterfaceOne {
//  6 |   uses InterfaceT
//    |

//----
//- diagnostics
// error: Expected Unexpected token in class body, found interface
//    ╭─[ class_spelling_error.baml:2:3 ]
//    │
//  2 │   interface strin
//    │   ────┬────  
//    │       ╰────── Expected Unexpected token in class body, found interface
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: field 'strin' is missing a type annotation
//    ╭─[ class_spelling_error.baml:2:13 ]
//    │
//  2 │   interface strin
//    │             ──┬──  
//    │               ╰──── field 'strin' is missing a type annotation
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: unresolved type: InterfaceT
//    ╭─[ class_spelling_error.baml:6:7 ]
//    │
//  6 │   uses InterfaceT
//    │       ─────┬─────  
//    │            ╰─────── unresolved type: InterfaceT
//    │ 
//    │ Note: Error code: E0002
// ───╯
