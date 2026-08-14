function UseUnknown() -> int {
  unknown_var + 1
}

//----
//- diagnostics
// E0003
//
//   × unresolved name: `unknown_var`
//    ╭─[unknown_variable.baml:2:3]
//  2 │   unknown_var + 1
//    ·   ───────────
//    ╰────
