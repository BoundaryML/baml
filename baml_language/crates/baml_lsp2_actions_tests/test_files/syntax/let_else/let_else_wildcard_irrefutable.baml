// Wildcard pattern matches everything → else is dead → warn (E0114).
function WildcardIrrefutable(x: int) -> int {
  let _ = x else { return 0; };
  x
}

//----
//- diagnostics
// warning: irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    ╭─[ let_else_let_else_wildcard_irrefutable.baml:3:3 ]
//    │
//  3 │   let _ = x else { return 0; };
//    │   ──┬──
//    │     ╰──── irrefutable `let … else` pattern; the `else` branch is unreachable — use a plain `let` binding instead
//    │
//    │ Note: Error code: E0114
// ───╯
