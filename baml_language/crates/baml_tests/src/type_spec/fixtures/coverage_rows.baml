// Coverage rows: what a match arm covers, judged against the matrix's
// canonical columns. Each case pins one way a row could claim too much
// (unsound acceptance) or too little (a false non-exhaustive).

// A written union type tests each of its alternatives: `true | null`
// covers the `true` half of the `bool` member.
type CrTrueOrNull = true | null

function cr_union_type_test(x: bool | null) -> int {
    let r = match (x) {
     // ^ int
        CrTrueOrNull => 1,
        false => 2
    }
    r
}

// A one-variant enum type test is a single row, never a one-way `or`.
enum CrOnly {
    One
}

function cr_one_variant_enum(x: unknown) -> int {
    let r = match (x) {
     // ^ int
        CrOnly => 1,
        _ => 2
    }
    r
}

// An interface destructure covers every implementing member of a union.
interface CrHasV {
    v int
}

class CrIa {
    v int

    implements CrHasV {}
}

class CrIb {
    v int

    implements CrHasV {}
}

function cr_interface_over_implementors(x: CrIa | CrIb) -> int {
    let r = match (x) {
     // ^ int
        CrHasV { v: _ } => 1
    }
    r
}

// A sub-pattern of the wrong shape covers nothing: the `string` slot
// is not a pair.
function cr_nested_tuple_shape(t: ((int, int), int) | (string, int)) -> int {
    let r = match (t) { ((_, _), _) => 1 };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

// A typed rest claims only the list members its type admits.
function cr_typed_rest(xs: int[] | string[]) -> int {
    let r = match (xs) { [..let rest: int[]] => 1 };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

function cr_typed_rest_one_list(xs: (int | string)[]) -> int {
    let r = match (xs) { [..let rest: int[]] => 1 };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

function cr_untyped_rest(xs: int[] | string[]) -> int {
    let r = match (xs) {
     // ^ int
        [..] => 1
    }
    r
}

// A hole in a written type covers what its solution proves: `x` solves
// to `CrBox<int>`, so the `CrBox<string>` member is missing.
class CrBox<T> {
    v T
}

function cr_hole_solved(b: CrBox<int> | CrBox<string>) -> int {
    let r = match (b) { let x: CrBox<_> => { let t: CrBox<int> = x; 1 } };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

// A hole nothing solves proves nothing: the row covers no member.
function cr_hole_unsolved(b: CrBox<int> | CrBox<string>) -> int {
    let r = match (b) { let x: CrBox<_> => 1 };
     // ^ !error
//          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ non-exhaustive
    r
}

// `_` after a binding is the wildcard sub-pattern, not a type hole: it
// covers everything.
function cr_wildcard_ascription(b: bool) -> int {
    let r = match (b) {
     // ^ int
        let v: _ => 1
    }
    r
}

// A written field union is its column's canonical form: `true | false`
// is `bool`, so the two literal arms cover it.
class CrFlag {
    value true | false
}

function cr_literal_union_field(f: CrFlag) -> int {
    let r = match (f) {
     // ^ int
        CrFlag { value: true } => 1,
        CrFlag { value: false } => 2
    }
    r
}

interface CrHasFlag {
    value true | false
}

function cr_literal_union_interface_field(f: CrHasFlag) -> int {
    let r = match (f) {
     // ^ int
        CrHasFlag { value: true } => 1,
        CrHasFlag { value: false } => 2
    }
    r
}

class CrPick {
    value "a" | "b"
}

function cr_string_union_field(p: CrPick) -> int {
    let r = match (p) {
     // ^ int
        CrPick { value: "a" } => 1,
        CrPick { value: "b" } => 2
    }
    r
}
