// Diagnostics for tuples. Each function isolates one error.

// Tuples are immutable: assigning to an element is rejected.
function assign_element() -> int {
    let t = (1, 2);
    t[0] = 5;
    t[0]
}

// Compound assignment is a mutation too, and the index's eligibility does not
// matter: any write is rejected.
function compound_assign_element(i: int) -> int {
    let t = (1, 2);
    t[0] += 1;
    t[i] = 3;
    t[0]
}

// A static index past the tuple's arity is out of range.
function index_out_of_range(t: (int, string)) -> int {
    t[2]
}

// So is a negative one, folded or not.
function negative_index(t: (int, string)) -> int {
    t[-1] + t[1 - 2]
}

// Element access on a non-tuple.
function index_non_tuple(x: int) -> int {
    x[0]
}

// Each element has its own type, so an unconstrained `int` index is rejected.
function non_static_index(t: (int, int), i: int) -> int {
    t[i] + t[i + 1]
}

// A literal-typed index that names a missing slot is out of range.
function literal_typed_out_of_range(t: (int, int), i: 2) -> int {
    t[i]
}

// A union of literals does not name one slot.
function literal_union_index(t: (int, int), i: 0 | 1) -> int {
    t[i]
}

// A non-integer index is an index-type error.
function string_index(t: (int, int)) -> int {
    t["0"]
}

// A nullable index is not an integer either, even through `?.[`.
function nullable_index(t: (int, int)?, i: 0?) -> int? {
    t?.[i]
}

// A union of tuples reads only the slots every variant has.
function union_missing_slot(t: (int, bool) | (string, bool, int)) -> int {
    t[2]
}

// Every member of an indexed union must be a tuple.
function union_with_list(t: (int, int) | int[]) -> int {
    t[0]
}

// A nullable tuple needs `?.[` to read a slot.
function nullable_receiver(t: (int, int)?) -> int {
    t[0]
}

// `?.[` on a tuple that cannot be null is unnecessary: a nullable ELEMENT is
// handled with postfix `?` on the element, not by optional indexing.
function needless_optional_index(t: (int?, int)) -> int? {
    t?.[0]
}

// Arity is part of the type.
function wrong_arity() -> (int, int) {
    (1, 2, 3)
}

// The same through a binding: both tuple types show.
function wrong_arity_binding(t: (int, string, bool)) -> (int, string) {
    t
}

// Elementwise mismatch.
function wrong_element() -> (int, string) {
    (1, 2)
}

// Tuples and lists are disjoint.
function tuple_is_not_list() -> int[] {
    (1, 2)
}

// A tuple match missing a case is non-exhaustive.
function non_exhaustive(a: bool, b: bool) -> int {
    match (a, b) {
        (true, let _b: true) => 1,
        (false, _) => 2,
    }
}

// A tuple pattern of the wrong arity can never match.
function wrong_arity_pattern(t: (int, int)) -> int {
    match (t) {
        (let a, let b, let c) => a,
        _ => 0,
    }
}

// Exhaustiveness witnesses keep slot positions.
function non_exhaustive_nested(t: (bool, (bool, int?))) -> int {
    match (t) {
        (true, _) => 1,
        (false, (true, _)) => 2,
        (false, (false, let _n: null)) => 3,
    }
}

// Tuples are invariant: an existing tuple does not fit a tuple type with
// wider slots, though a fresh literal is built at them.
function existing_tuple_not_widened() -> (int?, string) {
    let t: (int, string) = (1, "a");
    let fresh: (int?, string) = (1, "a");
    t
}

// A product of unions and the union of its products are unrelated, in both
// directions, and nested singletons are kept.
function product_not_union(t: (int | string, bool)) -> (int, bool) | (string, bool) {
    t
}

function union_not_product(t: (int, bool) | (string, bool)) -> (int | string, bool) {
    t
}

function nested_product_not_union(t: ((int | string,), bool)) -> ((int,), bool) | ((string,), bool) {
    t
}

// Invariant positions keep the distinction too.
class TpCell<T> {
    value: T,
}

function product_union_invariant(
    c: TpCell<(int | string, bool)>,
) -> TpCell<(int, bool) | (string, bool)> {
    c
}

// Two same-arity tuple arms give a fresh literal no slot context, so it is
// built at its own widened type, which equals neither arm. One tuple arm, or
// a matching arm, is fine.
function ambiguous_same_arity_literal() -> int {
    let a: (int?, string) | int = (1, "a");
    let b: (int?, string) | (string, int) = (1, "a");
    let c: (int?, string) | (int, string) = (1, "a");
    let first: (int?, string) = (1, "a");
    let d: (int?, string) | (string, int) = first;
    0
}

// A tuple made only of types tests the tuple's carried type; one that no
// tuple of the scrutinee's type can carry is an error, in a match arm, a
// binding, a nested element, an `is` test, and through an alias.
type TpZeroPair = (0, 0);

function type_test_never_matches(t: (int, int), u: (int, int)) -> bool {
    let nested = match ((t, 1)) {
        ((0, 0), _) => true,
        _ => false,
    };
    let held = (t, 1);
    match (t) {
        (0, 0) => true,
        let z: (0, 0) => true,
        let a: TpZeroPair => true,
        _ => nested || held[0] is (0, 0) || u is (int?, int),
    }
}

// Type-test arms check carried types, so they do not decompose: `(int, bool)`
// and `(string, bool)` never match a `(int | string, bool)` and leave it
// uncovered. Element tests decompose and cover it.
function type_test_arms_do_not_cover(t: (int | string, bool)) -> int {
    match (t) {
        (int, bool) => 1,
        (string, bool) => 2,
    }
}

function element_test_arms_cover(t: (int | string, bool)) -> int {
    match (t) {
        (int, _) => 1,
        (string, _) => 2,
    }
}

// A whole-tuple type test narrows a union of tuples to one member.
function tp_takes_wide(t: (int?, bool)) -> int { 0 }
function tp_takes_narrow(t: (int, bool)) -> int { 0 }

function type_test_narrows(t: (int?, bool) | (int, bool)) -> int {
    match (t) {
        let r: (int?, bool) => tp_takes_wide(r),
        let r: (int, bool) => tp_takes_narrow(r),
    }
}

// An element test removes only arms whose slot type it is disjoint from, and
// keeps the rest whole: never a refined tuple type.
function arm_eliminated(t: (int, bool) | (string, bool)) -> int {
    match (t) {
        (let n: int, _) => tp_takes_narrow(t),
        _ => 0,
    }
}

function arm_overlaps(t: (int?, bool) | (int, bool)) -> int {
    match (t) {
        (let n: int, _) => tp_takes_narrow(t),
        _ => 0,
    }
}

function arm_open<T>(t: (T, bool) | (int, bool)) -> int {
    match (t) {
        (let n: int, _) => tp_takes_narrow(t),
        _ => 0,
    }
}

function single_tuple_not_refined(t: (int | string, bool)) -> int {
    match (t) {
        (let n: int, _) => tp_takes_narrow(t),
        _ => 0,
    }
}

// A tuple pattern on `unknown` does not narrow it to a tuple of unknowns,
// which a tuple carrying concrete slot types is not.
function tp_takes_unknowns(pair: (unknown, unknown)) -> int { 0 }

function unknown_not_narrowed(v: unknown) -> int {
    match (v) {
        (let a, let b) => tp_takes_unknowns(v),
        _ => 1,
    }
}

// A recursive alias inside an optional is matched through: the witness names
// the tuple's slots, not the alias.
type TpBits = (bool, TpBits?);

function non_exhaustive_recursive_alias(bits: TpBits?) -> int {
    match (bits) {
        null => 0,
        (true, _) => 1,
        (false, let _rest: null) => 2,
    }
}

// A store through an optional chain is still a tuple mutation.
class TpHolder {
    pair: (int, int),
}

function optional_chain_assign(t: (int, int)?, h: TpHolder?) -> int {
    t?.[0] = 5;
    t?.[1] += 1;
    h?.pair[0] = 2;
    0
}

// A `let` binding widens its literal, so it is a runtime index; only a
// `const` keeps its value.
function let_index_widens(t: (int, string)) -> int {
    let i = 0;
    t[i]
}

// A `const` index is static only while its value is known: a loop that
// reassigns it havocs the value, and a closure that captures it never sees it.
function const_index_reassigned_in_loop(t: (int, int)) -> int {
    const i = 0;
    let total = 0;
    while (total < 2) {
        total += t[i];
        i = 1;
    }
    total
}

function const_index_captured(t: (int, int)) -> int {
    const i = 0;
    let read = () -> int { t[i] };
    read()
}

// An annotated global is an `int`, not its initializer's value.
const TP_WIDE_INDEX: int = 0;

function annotated_global_index(t: (int, int)) -> int {
    t[TP_WIDE_INDEX]
}

// There is no destructuring assignment: bind with `let (let a, let b) = pair`.
function destructuring_assignment(pair: (int, int)) -> int {
    let a = 0;
    let b = 0;
    (a, b) = pair;
    a + b
}

// Bare names in a tuple pattern are type patterns, not bindings.
function bare_name_elements(pair: (int, int)) -> int {
    let (a, b) = pair;
    0
}
