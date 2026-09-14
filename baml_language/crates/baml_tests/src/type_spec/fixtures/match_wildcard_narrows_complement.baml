//
// B-774: https://linear.app/boundaryml2/issue/B-774
// Each unguarded arm refutable by type alone subtracts what it matched,
// so the `_` arm sees the complement (`string`), not the full union -
// the catch-residual mechanism generalized to match. TIR keeps the full
// union in the `_` arm.
function f(v: int | string, d: string) -> string {
    match (v) {
        let n: int => d,
        _ => v,
          // ^ string
    }
}
