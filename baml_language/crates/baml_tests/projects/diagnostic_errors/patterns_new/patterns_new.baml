// Pattern error cases: malformed Or-bindings, name mismatches, etc.

// ============================================================================
// Or-pattern alternatives must bind the same name set
// ============================================================================

// Single binding name differs across branches.
function or_different_names_two(v: int) -> int {
    match (v) {
        (let x) | (let y) => 1
    }
}

// Three branches, all different names.
function or_different_names_three(v: int) -> int {
    match (v) {
        let x | let y | let z => 1
    }
}

// One branch binds a name, the other does not.
function or_one_binds_one_doesnt(v: int) -> int {
    match (v) {
        (let x) | _ => 1
    }
}

// Binding name + sibling branch with a different binding name in a chain.
function or_mixed_names_in_chain(v: int | string) -> int {
    match (v) {
        (let x: int) | (let y: string) => 1
    }
}

// ============================================================================
// Same name across branches but with conflicting narrows — should not compile.
// ============================================================================

function or_same_name_distinct_narrows(v: int | string) -> int {
    match (v) {
        (let x: int) | (let x: string) => 1
    }
}

// Two bindings per Or branch with conflicting narrows on the SECOND name —
// chain has [Bind(a), Bind(b), Type] so both `a` and `b` get the same
// effective type per branch. With only `first_names.first()` checked, we'd
// miss the mismatch when only the first name happens to agree across
// branches. The current check picks `a` and compares branch types, which
// catches the mismatch when both names share the chain's effective type.
function or_multiple_binds_diff_narrows(v: int | string) -> int {
    match (v) {
        (let a: let b: int) | (let a: let b: string) => 1
    }
}

// ============================================================================
// Array destructuring refutability in irrefutable contexts
// ============================================================================

class Team {
    scores int[]
}

// Empty array patterns can fail when the input is non-empty.
function refutable_let_empty_array(xs: int[]) -> int {
    let [] = xs;
    1
}

// Prefix patterns can fail when the input is too short.
function refutable_let_array_prefix(xs: int[]) -> int {
    let [let first, ..] = xs;
    first
}

// For-loop bindings are irrefutable contexts too.
function refutable_for_array_prefix(rows: int[][]) -> int {
    let total = 0;
    for (let [let first, ..] in rows) {
        total += first
    };
    total
}

// Refutability propagates through class fields.
function refutable_for_class_field_array_prefix(teams: Team[]) -> int {
    let total = 0;
    for (let Team { scores: [let first, ..] } in teams) {
        total += first
    };
    total
}

// ============================================================================
// Class destructure unreachability: bare `Class { }` covers every value of
// `Class`, so any later arm narrowing the same class is unreachable.
// ============================================================================

class Bag {
    contents int
}

function class_destructure_unreachable_after_bare(b: Bag) -> int {
    match (b) {
        Bag { } => 1,
        Bag { contents: int } => 2
    }
}

// ============================================================================
// Class destructure non-exhaustiveness: arms narrow on a union-typed field
// but miss one branch. The matrix should recurse into the field column and
// report the missing variant (`int`).
// ============================================================================

class Mixed {
    value int | float | bool
}

function class_destructure_field_union_missing_int(m: Mixed) -> int {
    match (m) {
        Mixed { value: float } => 1,
        Mixed { value: bool } => 2
    }
}

// Multi-field class where each field is a union. Arms cover three of the
// four (kind, payload) combinations; the missing one (`Pair { kind: bool,
// payload: string }`) should appear in the diagnostic with both field names.
class Pair {
    kind int | bool
    payload int | string
}

function class_destructure_two_field_union_missing_combination(p: Pair) -> int {
    match (p) {
        Pair { kind: int, payload: int } => 1,
        Pair { kind: int, payload: string } => 2,
        Pair { kind: bool, payload: int } => 3
    }
}

// ============================================================================
// Ported from rustc tests/ui/pattern/usefulness — exhaustiveness/witness shapes
// (skipping cases that need `..rest @` named-rest bindings)
// ============================================================================

// rustc: `enum_with_single_missing_variant` —
// missing one enum variant is reported by name.
enum Color3 {
    Red
    Green
    Blue
}

function enum_with_single_missing_variant(c: Color3) -> int {
    match (c) {
        Color3.Green => 1,
        Color3.Blue => 2
    }
}

// rustc: `enum_with_multiple_missing_variants` —
// multiple missing variants are listed.
enum Direction4 {
    North
    East
    South
    West
}

function enum_with_multiple_missing_variants(d: Direction4) -> int {
    match (d) {
        Direction4.North => 1
    }
}

// rustc: `enum_with_excessive_missing_variants` — 12 variants, 1 covered.
enum ExcessiveEnum {
    First
    Second
    Third
    Fourth
    Fifth
    Sixth
    Seventh
    Eighth
    Ninth
    Tenth
    Eleventh
    Twelfth
}

function enum_with_excessive_missing_variants(e: ExcessiveEnum) -> int {
    match (e) {
        ExcessiveEnum.First => 1
    }
}

// rustc: `enum_struct_variant` analog. Class with a bool field; arms cover
// every shape with `field: false`, leaving `field: true` uncovered. Witness
// should mention the missing bool value.
class Flagged {
    flag bool
    note string
}

function class_field_bool_narrowing_missing_true(f: Flagged) -> int {
    match (f) {
        Flagged { flag: false, note: "zero" } => 1,
        Flagged { flag: false, note: string } => 2
    }
}

// rustc: `struct_with_a_nested_enum_and_vector` analog (without the fixed-
// length array piece, which BAML doesn't model). Class with bool + optional;
// arms cover three of the four (bool × null/non-null) combos.
class Box2 {
    first bool
    second int[]?
}

function nested_class_optional_missing_combination(b: Box2) -> int {
    match (b) {
        Box2 { first: true, second: null } => 1,
        Box2 { first: true, second: int[] } => 2,
        Box2 { first: false, second: null } => 3
    }
}

// rustc: `match-non-exhaustive` — single literal arm against int.
function int_literal_non_exhaustive(n: int) -> int {
    match (n) {
        1 => 1
    }
}

// ============================================================================
// Ported from rustc or-patterns/exhaustiveness-non-exhaustive
// ============================================================================

// `(0 | 1, 2 | 3)` against `(int, int)` is non-exhaustive — there are int
// values outside `0|1` for the first slot. Modeled here with a class.
class IntPair {
    a int
    b int
}

function or_pattern_two_field_non_exhaustive(p: IntPair) -> int {
    match (p) {
        IntPair { a: 0 | 1, b: 2 | 3 } => 1
    }
}

// `(None | Some(0 | 1),)` against `Option<u8>` — Some(2..) is missing. BAML
// equivalent: an optional int with literal-only Or covering 0 and 1; any
// other int is missing.
function or_pattern_optional_int_non_exhaustive(x: int?) -> int {
    match (x) {
        null => 0,
        0 | 1 => 1
    }
}

// ============================================================================
// Ported from rustc slice-patterns-exhaustiveness / slice-patterns-reachability
// (rest binding `..rest` skipped — wildcard `..` only)
// ============================================================================

// `[true, ..]` + `[.., false]` doesn't cover `[false, true]` for arrays of
// length ≥ 2.
function slice_prefix_or_suffix_non_exhaustive(xs: bool[]) -> int {
    match (xs) {
        [] => 0,
        [true, ..] => 1,
        [.., false] => 2
    }
}

// rustc: `[true, ..]` then `[true, ..]` — second arm unreachable.
function slice_repeated_prefix_unreachable(xs: bool[]) -> int {
    match (xs) {
        [true, ..] => 1,
        [true, ..] => 2,
        _ => 3
    }
}

// rustc: `[.., true]` then `[.., true]` — second arm unreachable.
function slice_repeated_suffix_unreachable(xs: bool[]) -> int {
    match (xs) {
        [.., true] => 1,
        [.., true] => 2,
        _ => 3
    }
}

// rustc: `[false, .., true]` then `[false, true]` — second arm covered by
// the first when length is 2.
function slice_prefix_suffix_subsumes_fixed(xs: bool[]) -> int {
    match (xs) {
        [false, .., true] => 1,
        [false, true] => 2,
        _ => 3
    }
}
