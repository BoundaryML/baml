// R3 (S15 residue): `if let PAT = scrutinee { then } else { else }`: the pattern types
// its bindings (the S10 walk - array/rest destructures included), the
// then-branch sees the scrutinee narrowed to the matched type, the
// else-branch the residual; branch join and divergence follow the If
// discipline.
function f(xs: int[]) -> int throws never {
    if let [let a, ..let r] = xs {
        let k = r;
         // ^ int[]
        a
    } else {
        0
    }
}

function g(v: int | string) -> int throws never {
    if let s: string = v {
        let t = s;
         // ^ string
        0
    } else {
        let w = v;
         // ^ int
        1
    }
}
