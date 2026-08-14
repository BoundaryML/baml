// Array destructure in let-else: bare identifiers inside a pattern are
// TYPE patterns by default, so the binding names need explicit `let`
// markers. The whole pattern is refutable on `int[]` because the array
// could be shorter than two elements.
function FirstTwo(xs: int[]) -> int {
  let [let a, let b] = xs else { return -1; };
  a + b
}
