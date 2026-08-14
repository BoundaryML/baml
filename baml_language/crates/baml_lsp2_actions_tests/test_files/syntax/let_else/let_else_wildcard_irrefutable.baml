// Wildcard pattern matches everything → else is dead → warn (E0114).
function WildcardIrrefutable(x: int) -> int {
  let _ = x else { return 0; };
  x
}

//----
//- diagnostics
// <no-diagnostics-expected>
