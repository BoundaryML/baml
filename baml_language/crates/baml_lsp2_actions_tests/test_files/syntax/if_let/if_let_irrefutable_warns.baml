// `if let` with an irrefutable pattern should warn — `else` is unreachable.
function IrrefutablePlainBinding(x: int) -> int {
  if let v = x {
    v
  } else {
    0
  }
}

//----
//- diagnostics
// warning: irrefutable `if let` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    ╭─[ if_let_if_let_irrefutable_warns.baml:3:6 ]
//    │
//  3 │   if let v = x {
//    │      ──┬──
//    │        ╰──── irrefutable `if let` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    │
//    │ Note: Error code: E0112
// ───╯
