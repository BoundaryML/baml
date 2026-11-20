// On a column that may hold any type, every pattern narrows the value to
// a type before matching its structure: a type test, an ascribed list, a
// typed rest, a tuple, a class or an interface. Arms narrowing to the same
// type are judged together, a wider narrowing covers a narrower one, and
// different types stay apart.
class Box<T> {
    v: T,
}

interface HasV {
    v: int,
}

class Ia {
    v: int,

    implements HasV {}
}

// The first arm takes every `int[]`: the second is unreachable.
function test_covers_ascribed_list(x: unknown) -> int {
    match (x) {
        let xs: int[] => 1,
        [1]: int[] => 2,
        _ => 3,
    }
}

// The ascribed list takes every `int[]`: the type test after it is
// unreachable.
function ascribed_list_covers_test(x: unknown) -> int {
    match (x) {
        [..]: int[] => 1,
        let xs: int[] => 2,
        _ => 3,
    }
}

// A `string[]` reaches the second arm; the third repeats the first.
function distinct_lists_stay_apart(x: unknown) -> int {
    match (x) {
        let xs: int[] => 1,
        let ys: string[] => 2,
        [1]: int[] => 3,
        _ => 4,
    }
}

// `(_, _)` demands `(unknown, unknown)`. Container types are invariant, so
// that does not contain `(int, int)`: the second arm is not reported, the
// conservative answer.
function any_pair_and_int_pair(x: unknown) -> int {
    match (x) {
        (_, _) => 1,
        let t: (int, int) => 2,
        _ => 3,
    }
}

// An `(int, int)` is not every pair: `("s", "s")` reaches the second arm.
function int_pair_leaves_other_pairs(x: unknown) -> int {
    match (x) {
        let t: (int, int) => 1,
        (_, _) => 2,
        _ => 3,
    }
}

function test_covers_class_destructure(x: unknown) -> int {
    match (x) {
        let b: Box<int> => 1,
        Box<int> { v: 1 } => 2,
        _ => 3,
    }
}

// Every `Ia` is a `HasV`.
function interface_covers_implementor(x: unknown) -> int {
    match (x) {
        let h: HasV => 1,
        Ia { v: 1 } => 2,
        _ => 3,
    }
}

function int_covers_literal(x: unknown) -> int {
    match (x) {
        let n: int => 1,
        1 => 2,
        _ => 3,
    }
}

function bool_covers_true(x: unknown) -> int {
    match (x) {
        let b: bool => 1,
        true => 2,
        _ => 3,
    }
}

// A literal is not every `int`.
function literal_leaves_int(x: unknown) -> int {
    match (x) {
        1 => 1,
        let n: int => 2,
        _ => 3,
    }
}

// A rigid type parameter and `int` stay apart in both orders.
function rigid_and_int<T>(x: unknown) -> int {
    match (x) {
        let t: T => 1,
        let n: int => 2,
        _ => 3,
    }
}

function int_and_rigid<T>(x: unknown) -> int {
    match (x) {
        let n: int => 1,
        let t: T => 2,
        _ => 3,
    }
}

// In a type-parameter column the same rules hold.
function type_var_column<T>(x: T) -> int {
    match (x) {
        let xs: int[] => 1,
        [1]: int[] => 2,
        _ => 3,
    }
}

// A type-parameter member of a union narrows the same way.
function type_var_member<T>(x: T | null) -> int {
    match (x) {
        null => 0,
        let xs: int[] => 1,
        [1]: int[] => 2,
        _ => 3,
    }
}

// No narrowing covers the column.
function narrowings_without_wildcard(x: unknown) -> int {
    match (x) {
        let xs: int[] => 1,
        let n: int => 2,
    }
}
