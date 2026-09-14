// Aggregate literals adopt through expected UNIONS (json)
//
// (TIR renders the annotated binding as the EXPANDED alias union;
// alias-preserving renders are this engine's deliberate design.)
//
// Arrays/maps are invariant (spec: Variance), so `int[]` is NOT a
// subtype of `json`. `let j: json = [1, 2, 3]` typechecks because the
// LITERAL adopts the expected union's unique list member - it types
// as `json[]` outright, each element checked against `json`
// (expected-type propagation through aliases and unions; TS/TIR
// behavior). Same rule for map literals via `map<string, json>`.
function ja() -> baml.json.json throws never {
    let j: baml.json.json = [1, 2, 3];
     // ^ baml.json.json
    j
}

function jm() -> baml.json.json throws never {
    let m: baml.json.json = {"key": {"nested": [1, true, null]}};
     // ^ baml.json.json
    m
}
