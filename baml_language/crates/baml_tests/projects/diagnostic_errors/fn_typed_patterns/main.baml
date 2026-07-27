// Every callable value is fully realized (it curries its complete type
// arguments at creation), but the runtime cannot yet faithfully reconstruct
// every callable's signature: bound methods reconstruct no type and closures
// from generic frames reconstruct coarsened ones (the stored signature erases
// generic positions). A runtime test against a function type would therefore
// silently misroute those callables, so a pattern claiming function-typed
// values can only be compiled where its runtime test is never emitted — the
// final arm of an exhaustive, guardless, non-Or `match` (coverage already
// proves every reaching value matches, so the test is elided). Every position
// below would need a real test and is rejected (E0155) instead.

// Dispatch by signature between two function-typed union members: the first
// arm's test must discriminate one function type from another, which is not
// reliable for every callable value today.
function dispatch_callback(cb: ((int) -> int throws never) | ((string) -> string throws never)) -> int {
    match (cb) {
        let f: (int) -> int throws never => f(0),
        (string) -> string throws never => 0
    }
}

// Same, generic: `(int) -> T` cannot be discriminated from `(string) -> T`
// at runtime either.
function unwrap_typed<T>(cb: ((int) -> T throws never) | ((string) -> T throws never)) -> T {
    match (cb) {
        let f: (int) -> T throws never => f(0),
        let f: (string) -> T throws never => f("")
    }
}

// A function-typed arm followed by a wildcard: the non-final arm's test is
// emitted, so it is rejected even though its own member is covered.
function dispatch_with_wildcard(cb: ((int) -> int throws never) | ((string) -> int throws never)) -> int {
    match (cb) {
        (int) -> int throws never => 1,
        _ => 0
    }
}

// A function-typed arm before the member-consuming arm: legal in the other
// order (`null` first — the function arm becomes the elided final arm).
function fn_arm_not_last(cb: ((int) -> int throws never)?) -> int {
    match (cb) {
        let f: (int) -> int throws never => f(1),
        null => 0
    }
}

// A guard forces the final arm's test to be emitted. (Guarded arms also do
// not count toward coverage, so this match is non-exhaustive on top.)
function fn_arm_guarded(cb: (int) -> int throws never) -> int {
    match (cb) {
        let f: (int) -> int throws never if true => f(1)
    }
}

// A function type as one union-member alternative of a tested (non-final)
// pattern claims function values all the same.
function fn_union_member_arm(v: int | ((int) -> int throws never) | string) -> string {
    match (v) {
        let x: ((int) -> int throws never) | int => "either",
        let s: string => s
    }
}

// `is` always evaluates its test; against a function type it would answer
// `false` even for values that ARE members of the written type.
function fn_is(cb: (int) -> int throws never) -> bool {
    cb is (int) -> int throws unknown
}

// `if let` always evaluates its test.
function fn_if_let(cb: (int) -> int throws never) -> int {
    if let f: (int) -> int throws unknown = cb { f(1) } else { 0 }
}
