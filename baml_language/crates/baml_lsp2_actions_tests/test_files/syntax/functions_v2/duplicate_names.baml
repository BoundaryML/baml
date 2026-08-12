function Bar {
  input string
  output string
}

function Bar(a: string, b: int | bool) -> int {
  client Bar
  prompt `fa`
}

//----
//- diagnostics
// E0010
//
//   × invalid syntax
//    ╭─[functions_v2_duplicate_names.baml:1:14]
//  1 │ function Bar {
//    ·              ┬
//    ·              ╰── old-style function syntax; use `function Name(params...) -> ReturnType { ... }`
//    ╰────
// E0001
//
//   × mismatched types
//    ╭─[functions_v2_duplicate_names.baml:7:10]
//  7 │   client Bar
//    ·          ─┬─
//    ·           ╰── expected `ai.Client`, found `() -> !error throws never`
//    ╰────
