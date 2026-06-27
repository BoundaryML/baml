// Irrefutable pattern in let-else — the else branch is unreachable, so
// suggest replacing with a plain `let` binding (E0114).
function IrrefutablePlainBinding(x: int) -> int {
  let v = x else { return 0; };
  v
}

//----
//- diagnostics
// warning: irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    ╭─[ let_else_let_else_irrefutable_warns.baml:4:3 ]
//    │
//  4 │   let v = x else { return 0; };
//    │   ──┬──
//    │     ╰──── irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    │
//    │ Note: Error code: E0114
// ───╯
