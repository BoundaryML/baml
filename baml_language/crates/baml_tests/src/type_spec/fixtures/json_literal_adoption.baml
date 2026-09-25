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
type JalDeepList0 = int[]
type JalDeepList1 = null | JalDeepList0
type JalDeepList2 = float | JalDeepList1
type JalDeepList3 = string | JalDeepList2
type JalDeepMap0 = map<string, int>
type JalDeepMap1 = null | JalDeepMap0
type JalDeepMap2 = float | JalDeepMap1
type JalDeepMap3 = string | JalDeepMap2

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

function accept_deep_list(value: bool[] | JalDeepList3) -> void throws never {
    null
}

function ja_fuel_exhaustion_is_ambiguous() -> void throws never {
    accept_deep_list([1])
}

function accept_deep_map(value: map<string, bool> | JalDeepMap3) -> void throws never {
    null
}

function jm_fuel_exhaustion_is_ambiguous() -> void throws never {
    accept_deep_map({"key": 1})
}
