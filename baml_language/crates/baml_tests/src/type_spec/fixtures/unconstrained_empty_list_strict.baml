// RULING (2026-08-07): unconstrained empty containers are rustc-strict
// (E0282's shape) - no annotation and no constraining use leaves the
// element variable unsolved, and it erases to the Error sentinel (S17
// renders "type annotations needed"). TIR is TS-lenient (`never[]` /
// `_[]`), so this fixture is intentionally ahead of TIR.
function uel_bare() -> int throws never {
    let e = []
     // ^ !error[]
    e.length()
}
function uel_hetero() -> int throws never {
    let total = 0
    for (let row in [[1], [2, 3], []]) {
        total += row.length()
    }
    total
}
