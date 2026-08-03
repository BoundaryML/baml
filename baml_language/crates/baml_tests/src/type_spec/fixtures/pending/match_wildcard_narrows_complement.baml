// pending: S10b match residual accumulation across arms
//
// B-774: https://linear.app/boundaryml2/issue/B-774
// Each arm subtracts what earlier unguarded arms provably matched, so
// the `_` arm sees the complement (`string`), not the full scrutinee
// union - the catch-residual mechanism generalized to match. TIR keeps
// the full union in the `_` arm.
// tir: fails
function f(v: int | string, d: string) -> string {
    match (v) {
        let n: int => d,
        _ => v,
          // ^ string
    }
}
