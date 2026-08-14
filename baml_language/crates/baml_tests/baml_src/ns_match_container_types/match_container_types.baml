// End-to-end tests for element-discriminating container type tests in `match`
// and `is`.
//
// Before the MIR/emit structural-routing fix, `int[]` and `string[]` both
// lowered their runtime type test to a single coarse `LIST` type tag (element
// type discarded), so a `string[]` value wrongly matched an `int[]` arm — and a
// `map<string,int>` arm accepted a `map<string,string>` value. The fix routes an
// element-discriminating container template through the VM's structural value
// matcher (`ConstValue::Type`), which relates generic-argument positions
// invariantly: `int[]` is not `string[]`, `map<string,int>` is not
// `map<string,string>`.
//
// Locals holding container values are fine inside functions; test bodies call
// top-level helpers (the VM local-boxing caveat only bites class instances).

// ─── 1. list element discrimination via `match` (2-arm chain) ─────────────────

function classify_list(x: int[] | string[]) -> string {
    match (x) {
        let a: int[] => "ints",
        let b: string[] => "strings",
    }
}

function classify_int_list_fn() -> string {
    classify_list([1, 2, 3])
}

test "match_container_int_list" {
    assert.equal(classify_int_list_fn(), "ints")
}

function classify_string_list_fn() -> string {
    classify_list(["a", "b"])
}

test "match_container_string_list" {
    assert.equal(classify_string_list_fn(), "strings")
}

// ─── 2. list element discrimination via `is` ──────────────────────────────────

function int_list_is_int_array_fn() -> bool {
    let x: int[] | string[] = [1, 2, 3];
    x is int[]
}

test "is_int_list_is_int_array" {
    assert.is_true(int_list_is_int_array_fn())
}

function string_list_is_int_array_fn() -> bool {
    let x: int[] | string[] = ["a", "b"];
    x is int[]
}

test "is_string_list_is_not_int_array" {
    assert.is_true(!string_list_is_int_array_fn())
}

// ─── 3. map value-type discrimination via `match` ─────────────────────────────

function classify_map(x: map<string, int> | map<string, string>) -> string {
    match (x) {
        let a: map<string, int> => "int values",
        let b: map<string, string> => "string values",
    }
}

function classify_int_map_fn() -> string {
    let m: map<string, int> = { "k": 1 };
    classify_map(m)
}

test "match_container_int_map" {
    assert.equal(classify_int_map_fn(), "int values")
}

function classify_string_map_fn() -> string {
    let m: map<string, string> = { "k": "v" };
    classify_map(m)
}

test "match_container_string_map" {
    assert.equal(classify_string_map_fn(), "string values")
}

// ─── 4. list element discrimination on the TypeTag switch path ────────────────
// With enough tag-eligible arms the lowering prefers the TypeTag jump-table
// switch (4+ *distinct* tags — colliding tags dedup before the count, so this
// needs five arms). Before the switch tag-sufficiency gate, `int[]` and
// `string[]` collapsed to one coarse LIST tag: the jump table kept only the
// first list arm and every list value jumped there, so a `string[]` value
// silently took the `int[]` arm. The gate detects the collision and falls back
// to the (element-precise) chain.

function classify_switch(x: int[] | string[] | int | string | bool) -> string {
    match (x) {
        let a: int[] => "ints",
        let b: string[] => "strings",
        let n: int => "int",
        let s: string => "string",
        let f: bool => "bool",
    }
}

function classify_switch_int_list_fn() -> string {
    classify_switch([1, 2, 3])
}

test "match_switch_int_list" {
    assert.equal(classify_switch_int_list_fn(), "ints")
}

function classify_switch_string_list_fn() -> string {
    classify_switch(["a", "b"])
}

test "match_switch_string_list" {
    assert.equal(classify_switch_string_list_fn(), "strings")
}

function classify_switch_prims_fn() -> string {
    classify_switch(42)
}

test "match_switch_prims_still_dispatch" {
    assert.equal(classify_switch_prims_fn(), "int")
}

// ─── 5. invariance pins ───────────────────────────────────────────────────────
// Generic-argument positions are invariant (TYPE_SYSTEM.md "Variance"): a list
// whose runtime element type is `int` is NOT a member of `(int | string)[]`,
// while a list constructed *at* element type `int | string` (the literal adopts
// the ascribed element type at construction) is. Static coverage agrees: each
// arm covers exactly its own union member, so both arms are reachable and the
// match is exhaustive without guards or a wildcard.

function classify_union_element(u: (int | string)[] | int[]) -> string {
    match (u) {
        let i: int[] => "ints",
        let m: (int | string)[] => "mixed",
    }
}

// The same match with the wider arm FIRST: the `(int | string)[]` arm claims
// (and tests) only its own union member, so an `int[]` value falls through to
// the `int[]` arm instead of being captured — and bound writable — at the
// wider element type (through which a later `push("x")` would corrupt the
// original `int[]`).
function classify_union_element_mixed_first(u: (int | string)[] | int[]) -> string {
    match (u) {
        let m: (int | string)[] => "mixed",
        let i: int[] => "ints",
    }
}

function int_list_vs_union_element_arm_fn() -> string {
    let a: int[] = [1, 2];
    classify_union_element(a)
}

test "match_invariance_int_list_not_member_of_union_element_arm" {
    assert.equal(int_list_vs_union_element_arm_fn(), "ints")
}

function union_element_constructed_matches_fn() -> string {
    let v: (int | string)[] = [1, 2];
    classify_union_element(v)
}

test "match_invariance_union_element_constructed_list_matches" {
    assert.equal(union_element_constructed_matches_fn(), "mixed")
}

function mixed_first_int_list_fn() -> string {
    let a: int[] = [1, 2];
    classify_union_element_mixed_first(a)
}

test "match_invariance_mixed_first_arm_does_not_capture_int_list" {
    assert.equal(mixed_first_int_list_fn(), "ints")
}

function mixed_first_union_list_fn() -> string {
    let v: (int | string)[] = [1, 2];
    classify_union_element_mixed_first(v)
}

test "match_invariance_mixed_first_arm_captures_union_list" {
    assert.equal(mixed_first_union_list_fn(), "mixed")
}

// ─── 6. `unknown[]` is invariant, not "any list" ──────────────────────────────
// Invariance applies to the top type like any other element type: `unknown[]`
// denotes lists whose element type is `unknown`, so an `int[]` value is NOT a
// member (the match-anything container position is the wildcard hole `_`, not
// the `unknown` type). A list constructed *at* element type `unknown` (the
// literal adopts the ascribed element type) IS a member.

function int_list_is_not_unknown_list_fn() -> bool {
    let a: int[] = [1, 2];
    // Widen to `unknown` (the top type — sound under any variance) before
    // testing. Tested directly on the `int[]`-typed local, TIR's covariant
    // container narrowing — a defect under removal; containers are never
    // covariant — rewrites the pattern to `int[]`, turning the very test we
    // want to pin into a tautology. `unknown ∩ unknown[]` keeps the written
    // pattern, and stays correct once narrowing is invariant (at which point
    // the direct form works too).
    let u: unknown = a;
    u is unknown[]
}

test "is_int_list_is_not_unknown_list" {
    assert.is_true(!int_list_is_not_unknown_list_fn())
}

function unknown_list_is_unknown_list_fn() -> bool {
    let v: unknown[] = [1, 2];
    v is unknown[]
}

test "is_unknown_constructed_list_is_unknown_list" {
    assert.is_true(unknown_list_is_unknown_list_fn())
}

// The same invariance through a match arm: an `unknown[]` arm does not capture
// an `int[]` value, and static coverage now agrees — user-written `unknown`
// is the real top type (`BuiltinUnknown`), invariant in element position like
// any other type, so the `unknown[]` arm covers only its own member and the
// `int[]` arm stays reachable without a guard. (Coverage's recovery escape
// counts only the `Unknown`/`Error` resolution-failure sentinels, not the
// user-written top type.)
function classify_unknown_vs_int(u: unknown[] | int[]) -> string {
    match (u) {
        let a: unknown[] => "unknowns",
        let i: int[] => "ints",
    }
}

function unknown_arm_rejects_int_list_fn() -> string {
    let a: int[] = [1, 2];
    classify_unknown_vs_int(a)
}

test "match_unknown_list_arm_rejects_int_list" {
    assert.equal(unknown_arm_rejects_int_list_fn(), "ints")
}

function unknown_arm_captures_unknown_list_fn() -> string {
    let v: unknown[] = [1, 2];
    classify_unknown_vs_int(v)
}

test "match_unknown_list_arm_captures_unknown_list" {
    assert.equal(unknown_arm_captures_unknown_list_fn(), "unknowns")
}
