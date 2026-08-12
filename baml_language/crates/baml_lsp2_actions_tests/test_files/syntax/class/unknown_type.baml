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
// E0002
//
//   × unresolved type: Bar
//    ╭─[class_unknown_type.baml:3:9]
//  3 │   prop2 Bar
//    ·         ───
//    ╰────
// E0002
//
//   × unresolved type: Bar
//    ╭─[class_unknown_type.baml:1:1]
//  1 │ class InterfaceTwo {
//    · ▲
//    ╰────
