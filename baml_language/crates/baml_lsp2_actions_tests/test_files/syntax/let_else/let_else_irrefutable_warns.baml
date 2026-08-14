// Irrefutable pattern in let-else — the else branch is unreachable, so
// suggest replacing with a plain `let` binding (E0114).
function IrrefutablePlainBinding(x: int) -> int {
  let v = x else { return 0; };
  v
}

//----
//- diagnostics
// <no-diagnostics-expected>
