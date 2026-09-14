// A8 (S15 ledger): array rest patterns turned out to WORK in every
// corpus variant - the ledger's `!error` entries at rest-pattern sites
// were cascades of the missing index expressions (`r[0]`) inside the
// arms. Pinned as a regression fixture: trailing, middle, rest-only in a destructuring let, and nested rest
// per TIR's array-pattern shape rows; usefulness through the lifted
// engine (rustc slice patterns are the reference).
function f(xs: int[]) -> int throws never {
    match (xs) {
        [let a, ..let r, let z] => {
            let m = r;
             // ^ int[]
            a + z
        },
        [let a, ..let r] => {
            let n = r;
             // ^ int[]
            a
        },
        _ => 0,
    }
}

function g() -> int throws never {
    let [..let all] = [1, 2, 3];
    let k = all;
     // ^ int[]
    k.length()
}

function h(xss: int[][]) -> int throws never {
    match (xss) {
        [[..let inner], .._] => {
            let w = inner;
             // ^ int[]
            w.length()
        },
        _ => 0,
    }
}
