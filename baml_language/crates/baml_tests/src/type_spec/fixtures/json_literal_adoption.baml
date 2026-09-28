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
type JalWideList0 = (int | string)[]
type JalWideList1 = null | JalWideList0
type JalWideList2 = float | JalWideList1
type JalWideList3 = bool | JalWideList2
type JalWideList4 = JalWideList3
type JalWideList5 = JalWideList4 | null

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

function ja_deep_second_list_arm_is_ambiguous() -> void throws never {
    accept_deep_list([1])
}

function accept_deep_map(value: map<string, bool> | JalDeepMap3) -> void throws never {
    null
}

function jm_deep_second_map_arm_is_ambiguous() -> void throws never {
    accept_deep_map({"key": 1})
}

// Adoption does not depend on how many aliases spell the expected type:
// the unique list arm is found however deeply it is nested.
function ja_deep_unique_list_arm() -> JalWideList5 throws never {
    [1]
//  ^^^ (int | string)[]
}

// A VARIABLE does not adopt: only literals are built at the expected
// type. The map literal adopts `map<string, json>`, but `a` is already
// `int[]`, and invariance keeps `int[]` out of `json[]` (spec: Variance).
function wrap_int_array_in_json(a: int[]) -> baml.json.json throws never {
    {int_array: a}
//              ^ mismatch: expected baml.json.json, got int[]
}
