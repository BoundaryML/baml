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

// `json?` is `json | null`: the list/map arms sit behind the alias nested
// in the union, and literals still adopt them.
function accept_optional_json(j: baml.json.json?) -> baml.json.json? throws never {
    j
}

function jm_optional() -> baml.json.json? throws never {
    accept_optional_json({"key": {"nested": [1, true, null]}})
}

class JalOptionalJsonField {
    data baml.json.json?
}

function jm_optional_field() -> baml.json.json? throws never {
    JalOptionalJsonField { data: {"key": {"nested": [1, true, null]}} }.data
}

function ja_optional() -> baml.json.json? throws never {
    accept_optional_json([1, true, null])
}

// A VARIABLE does not adopt: only literals are built at the expected
// type. The map literal adopts `map<string, json>`, but `a` is already
// `int[]`, and invariance keeps `int[]` out of `json[]` (spec: Variance).
function wrap_int_array_in_json(a: int[]) -> baml.json.json throws never {
    {"int_array": a}
//                ^ mismatch: expected baml.json.json, got int[]
}
