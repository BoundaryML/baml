// Patterns: weird union and binding cases.

// Same typed binding repeated across both `|` branches with explicit parens.
function or_with_repeated_typed_binding(v: int | string) -> int {
    match (v) {
        (let x: int) | (let x: int) => x,
        let _ => 0
    }
}

// Same bare binding repeated across multiple `|` branches.
function or_with_repeated_bare_binding(v: int) -> int {
    match (v) {
        let x | let x | let x => x
    }
}

// Wildcard arm last; non-trivial chain narrow in the first arm.
function chain_narrow_then_wildcard(v: int | string | bool) -> int {
    match (v) {
        let n: int => n,
        _ => 0
    }
}

// Bare type pattern (no binding, no narrow) in match-arm position.
function bare_type_arm(v: int | string) -> int {
    match (v) {
        int => 1,
        string => 2
    }
}

// ============================================================================
// Function-type patterns
// ============================================================================

// Match on a single function type — arm narrows to the callable.
function call_int_callback(cb: (int) -> int) -> int {
    match (cb) {
        let f: (int) -> int throws unknown => f(10)
    }
}

// A union of two distinct function types bound whole by the final arm.
function bind_callback_union(cb: ((int) -> int throws never) | ((string) -> string throws never)) -> int {
    match (cb) {
        let f: ((int) -> int throws never) | ((string) -> string throws never) => 0
    }
}

// Bare union-typed pattern across two function-typed alternatives.
function bind_either_int_caller(cb: ((int) -> int throws never) | ((bool) -> int throws never)) -> int {
    match (cb) {
        let g: ((int) -> int throws never) | ((bool) -> int throws never) => 1
    }
}

// Generic function-type pattern. T is unified at the call site. The outer
// parens disambiguate the union — without them the parser binds `|` inside
// the return type, yielding `(int) -> (T | (string) -> T)`.
function unwrap_typed<T>(cb: ((int) -> T throws never) | ((string) -> T throws never)) -> int {
    match (cb) {
        let f: ((int) -> T throws never) | ((string) -> T throws never) => 0
    }
}

// ============================================================================
// Triple higher-order: function returning a function returning a function
// ============================================================================

// `f: (int) -> (int) -> (int) -> int` — three-deep curried int function.
// `g: (string) -> (string) -> string`   — two-deep curried string function.
// The match dispatches on which curried shape was passed and unwraps it.
function unwrap_curried(
    f: ((int) -> ((int) -> ((int) -> int throws never) throws never) throws never) | ((string) -> ((string) -> string throws never) throws never)
) -> int {
    match (f) {
        let any: ((int) -> ((int) -> ((int) -> int throws never) throws never) throws never) | ((string) -> ((string) -> string throws never) throws never) => 0
    }
}

// Match on a function that takes a callback. The arm narrows to the
// callback-accepting shape, then we synthesize an int callback inline.
function call_with_callback(
    runner: (((int) -> int throws never) -> int throws never) | (((string) -> int throws never) -> int throws never)
) -> int {
    match (runner) {
        let r: ((((int) -> int throws never) -> int throws never)) | ((((string) -> int throws never) -> int throws never)) => 0
    }
}

// ============================================================================
// Chain narrows + subtyping: each link must be a subtype of the next, so the
// chain widens left-to-right. The rightmost concrete narrow is the effective
// annotation.
// ============================================================================

// Literal-narrow widening to primitive: `1 <: int`.
function literal_widens_to_int(v: int) -> int {
    match (v) {
        let n: int => n,
    }
}

// Function-type widening: a narrower return type is a subtype of a wider one,
// so `(int) -> int <: (int) -> int | string`.
function fn_return_widens(cb: (int) -> int | string) -> int {
    match (cb) {
        (int) -> (int | string) throws unknown => 1,
    }
}

type Number = int | float
function int_widens_to_alias(v: Number) -> int {
    match (v) {
        let n: Number => 1,
    }
}

// ============================================================================
// Nested unions and chains in parentheses
// ============================================================================

// Paren-wrapped Or with each branch itself a chain.
function paren_chains_in_or(v: int) -> int {
    match (v) {
        ((let x: int)) | ((let x: int)) => 1
    }
}

function fn_pattern(cb: (int) -> int) -> int {
    match (cb) {
        let f: (int) -> int throws unknown => f(0)
    }
}

// ============================================================================
// for-let with crazy chains
// ============================================================================

// for-let with a single type narrow.
function for_let_with_narrow(items: int[]) -> int {
    let sum = 0;
    for (let n: int in items) {
        sum += n
    }
    sum
}

function for_let_ascription(items: int[]) -> int {
    let sum = 0;
    for (let n: int | string in items) {
        sum += 1
    }
    sum
}

// for-let with a function-type narrow.
function for_let_fn_narrow(items: ((int) -> int throws never)[]) -> int {
    let total = 0;
    for (let f: (int) -> int throws never in items) {
        total += f(0)
    }
    total
}

// ============================================================================
// catch arms with crazy chains
// ============================================================================

class AppError { code int }

function risky_error(mode: int) -> int throws AppError | string {
    if (mode == 0) {
        throw AppError { code: 1 }
    }
    throw "boom"
}

// Catch arm with a chain narrow of a union — binds the joined type.
function catch_chain_union(mode: int) -> int {
    risky_error(mode) catch (e) {
        AppError | string => 1
    }
}

// Catch arms with paren-wrapped Or chains, each branch a typed binding.
function catch_paren_or_chain(mode: int) -> int {
    risky_error(mode) catch (e) {
        (AppError) | (string) => 1
    }
}

// Catch arms with widening chain narrows (literal -> primitive -> union).
function catch_widening_chain(mode: int) -> int {
    risky_error(mode) catch (e) {
        let s: string => 1,
        let app: AppError => 2
    }
}

// ============================================================================
// Type unions with suffixes wrapped in many layers of parens
// ============================================================================

// Single-layer paren around a union with array suffix.
function nested_paren_union_array_one(items: (int | string)[]) -> int {
    items.length()
}

// Two paren layers: `((int | string)[])` is the same as `(int | string)[]`.
function nested_paren_union_array_two(items: ((int | string)[])) -> int {
    items.length()
}

// Three paren layers. Chain narrow on the outside.
function nested_paren_union_array_three(v: (((int | string)[])[])) -> int {
    v.length()
}

// Chained suffix: union with `[]` wrapped, then `?` outside.
function nested_paren_union_array_optional(v: ((int | string)[])?) -> int {
    match (v) {
        ((int | string)[])? => 1
    }
}

// In a match arm, deeply parenthesized type union as the pattern.
function match_nested_paren_union(v: (int | string)[]) -> int {
    match (v) {
        (((int | string)[])) => 1
    }
}

// ============================================================================
// Nested array/class destructuring with branch-local bindings
// ============================================================================

class PatternBucket {
    values int[]
}

class PatternMatrix {
    rows int[][]
}

//----
//- diagnostics
// <no-diagnostics-expected>
