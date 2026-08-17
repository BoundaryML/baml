function WithParams(a: int) -> string {
  `Value: ${a}`
}

function BadCall1() -> string {
  `${WithParams(a = 2, b = 2)}`
}

function BadCall2() -> string {
  `${WithParams("a")}`
}

function BadCall3() -> string {
  `${WithParams()}`
}

function BadCall4() -> string {
  `${Random(2)}`
}

//----
//- diagnostics
// E0005
//
//   × expected 1 argument(s), got 2
//    ╭─[template_string_bad_calls.baml:6:6]
//  6 │   `${WithParams(a = 2, b = 2)}`
//    ·      ────────────────────────
//    ╰────
// E0005
//
//   × unknown named argument `b`
//    ╭─[template_string_bad_calls.baml:6:6]
//  6 │   `${WithParams(a = 2, b = 2)}`
//    ·      ────────────────────────
//    ╰────
// E0001
//
//   × mismatched types
//     ╭─[template_string_bad_calls.baml:10:17]
//  10 │   `${WithParams("a")}`
//     ·                 ─┬─
//     ·                  ╰── expected `int`, found `"a"`
//     ╰────
// E0005
//
//   × expected 1 argument(s), got 0
//     ╭─[template_string_bad_calls.baml:14:6]
//  14 │   `${WithParams()}`
//     ·      ────────────
//     ╰────
// E0003
//
//   × unresolved name: `Random`
//     ╭─[template_string_bad_calls.baml:18:6]
//  18 │   `${Random(2)}`
//     ·      ──────
//     ╰────
