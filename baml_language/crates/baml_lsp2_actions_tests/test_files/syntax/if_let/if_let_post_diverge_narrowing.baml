// Post-diverge narrowing: when `if let P = x { return … }` and the
// then-branch always diverges, `x` should narrow to the complement type
// after the statement. Mirrors the same narrowing that `if x is P {
// return … }` (and other diverging if-then forms) gives. Caught by
// CodeRabbit on #3579.
class Ok {
  value string
}
class Err {
  message string
}

function ResidualNarrowAfterDiverge(r: Ok | Err) -> string {
  if let o: Ok = r {
    return o.value;
  }
  // `r` should be narrowed to `Err` here — `r.message` must resolve.
  r.message
}

//----
//- diagnostics
// <no-diagnostics-expected>
