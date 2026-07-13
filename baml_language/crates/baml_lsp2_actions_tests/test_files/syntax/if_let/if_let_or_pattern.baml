// `if let A | B = ...` — or-pattern alternatives must bind the same names
// across both sides; the binding's type joins both branches.
class Ok {
  value string
}
class Warn {
  value string
}
class Err {
  message string
}

// Both alternatives bind `s` to a class that has a `value` field. The
// binding's type is `Ok | Warn`, and `.value` is the field common to both.
function OrPatternBothBindValue(r: Ok | Warn | Err) -> string {
  if let s: Ok | Warn = r {
    s.value
  } else {
    r.message
  }
}

//----
//- diagnostics
// error: type `Ok | Warn` has no member `value`: its members implement no common interface that declares `value`
//     ╭─[ if_let_if_let_or_pattern.baml:17:7 ]
//     │
//  17 │     s.value
//     │       ──┬──
//     │         ╰──── type `Ok | Warn` has no member `value`: its members implement no common interface that declares `value`
//     │
//     │ Note: Error code: E0007
// ────╯
