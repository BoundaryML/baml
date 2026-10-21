// B-267 part 4 (https://linear.app/boundaryml2/issue/B-267): a stdlib
// combinator's effect generic (`map`'s E2) must resolve per call site
// and never leak unresolved into a non-generic caller's throws surface.
// Canary TIR passes this shape too (the ticket predates the lambda
// rework); hir_ty's effect vars default unconstrained effects to
// `never` and the declared `throws never` contract verifies - an
// AGREEMENT fixture pinning both engines.
function aggregate(xs: string[]) -> string[] {
    xs.map((r: string) -> string { r })
}

function caller(xs: string[]) -> string[] throws never {
    let r = aggregate(xs);
     // ^ string[]
    r
}
