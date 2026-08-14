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
// <no-diagnostics-expected>
