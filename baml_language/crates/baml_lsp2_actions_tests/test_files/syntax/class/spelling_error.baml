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
// E0002
//
//   × unresolved type: InterfaceT
//    ╭─[class_spelling_error.baml:1:1]
//  1 │ class InterfaceTwo {
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: strin
//    ╭─[class_spelling_error.baml:2:13]
//  2 │   interface strin
//    ·             ─────
//    ╰────
// E0002
//
//   × unresolved type: strin
//    ╭─[class_spelling_error.baml:1:1]
//  1 │ class InterfaceTwo {
//    · ▲
//    ╰────
// E0002
//
//   × unresolved type: InterfaceT
//    ╭─[class_spelling_error.baml:6:8]
//  6 │   uses InterfaceT
//    ·        ──────────
//    ╰────
