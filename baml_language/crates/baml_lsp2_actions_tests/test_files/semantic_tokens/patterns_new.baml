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
        let f: (int) -> int throws never => f(10)
    }
}

// Match on a union of two distinct function types; dispatch by signature.
function dispatch_callback(cb: ((int) -> int throws never) | ((string) -> string throws never)) -> int {
    match (cb) {
        let f: (int) -> int throws never => f(0),
        (string) -> string throws never => 0
    }
}

// Or-pattern across two function-typed alternatives that share the same return.
function call_either_int_caller(cb: ((int) -> int throws never) | ((bool) -> int throws never)) -> int {
    match (cb) {
        (int) -> int throws never => 1,
        (bool) -> int throws never => 2
    }
}

// Function-type narrow with chain-narrow position (`(int) -> T`).
function dispatch_with_wildcard(cb: ((int) -> int throws never) | ((string) -> int throws never)) -> int {
    match (cb) {
        (int) -> int throws never => 1,
        _ => 0
    }
}

// Generic function-type pattern. T is unified at the call site. The outer
// parens disambiguate the union — without them the parser binds `|` inside
// the return type, yielding `(int) -> (T | (string) -> T)`.
function unwrap_typed<T>(cb: ((int) -> T throws never) | ((string) -> T throws never)) -> T {
    match (cb) {
        let f: (int) -> T throws never => f(0),
        let f: (string) -> T throws never => f("")
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
        let triple: (int) -> ((int) -> ((int) -> int throws never) throws never) throws never => triple(1)(2)(3),
        let pair: (string) -> ((string) -> string throws never) throws never => 0
    }
}

// Match on a function that takes a callback. The arm narrows to the
// callback-accepting shape, then we synthesize an int callback inline.
function call_with_callback(
    runner: (((int) -> int throws never) -> int throws never) | (((string) -> int throws never) -> int throws never)
) -> int {
    match (runner) {
        let r: ((int) -> int throws never) -> int throws never => r((x: int) -> int { x + 1 }),
        let r: ((string) -> int throws never) -> int throws never => r((s: string) -> int { 0 })
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
        (int) -> (int | string) throws never => 1,
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
        let f: (int) -> int throws never => f(0)
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
//
//- semantic_tokens
// patterns_new.baml:1:1 (comment) len=43 "// Patterns: weird union and binding cases."
// patterns_new.baml:3:1 (comment) len=77 "// Same typed binding repeated across both `|` branches with explicit parens."
// patterns_new.baml:4:1 (keyword) len=8 "function"
// patterns_new.baml:4:10 (function) [declaration] len=30 "or_with_repeated_typed_binding"
// patterns_new.baml:4:41 (parameter) [declaration] len=1 "v"
// patterns_new.baml:4:44 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:4:48 (operator) len=1 "|"
// patterns_new.baml:4:50 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:4:61 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:5:5 (keyword) len=5 "match"
// patterns_new.baml:5:12 (parameter) len=1 "v"
// patterns_new.baml:6:10 (keyword) len=3 "let"
// patterns_new.baml:6:14 (variable) [declaration] len=1 "x"
// patterns_new.baml:6:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:6:22 (operator) len=1 "|"
// patterns_new.baml:6:25 (keyword) len=3 "let"
// patterns_new.baml:6:29 (variable) [declaration] len=1 "x"
// patterns_new.baml:6:32 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:6:40 (variable) len=1 "x"
// patterns_new.baml:7:9 (keyword) len=3 "let"
// patterns_new.baml:7:18 (number) len=1 "0"
// patterns_new.baml:11:1 (comment) len=59 "// Same bare binding repeated across multiple `|` branches."
// patterns_new.baml:12:1 (keyword) len=8 "function"
// patterns_new.baml:12:10 (function) [declaration] len=29 "or_with_repeated_bare_binding"
// patterns_new.baml:12:40 (parameter) [declaration] len=1 "v"
// patterns_new.baml:12:43 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:12:51 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:13:5 (keyword) len=5 "match"
// patterns_new.baml:13:12 (parameter) len=1 "v"
// patterns_new.baml:14:9 (keyword) len=3 "let"
// patterns_new.baml:14:13 (variable) [declaration] len=1 "x"
// patterns_new.baml:14:15 (operator) len=1 "|"
// patterns_new.baml:14:17 (keyword) len=3 "let"
// patterns_new.baml:14:21 (variable) [declaration] len=1 "x"
// patterns_new.baml:14:23 (operator) len=1 "|"
// patterns_new.baml:14:25 (keyword) len=3 "let"
// patterns_new.baml:14:29 (variable) [declaration] len=1 "x"
// patterns_new.baml:14:34 (variable) len=1 "x"
// patterns_new.baml:18:1 (comment) len=64 "// Wildcard arm last; non-trivial chain narrow in the first arm."
// patterns_new.baml:19:1 (keyword) len=8 "function"
// patterns_new.baml:19:10 (function) [declaration] len=26 "chain_narrow_then_wildcard"
// patterns_new.baml:19:37 (parameter) [declaration] len=1 "v"
// patterns_new.baml:19:40 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:19:44 (operator) len=1 "|"
// patterns_new.baml:19:46 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:19:53 (operator) len=1 "|"
// patterns_new.baml:19:55 (type) [defaultLibrary] len=4 "bool"
// patterns_new.baml:19:64 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:20:5 (keyword) len=5 "match"
// patterns_new.baml:20:12 (parameter) len=1 "v"
// patterns_new.baml:21:9 (keyword) len=3 "let"
// patterns_new.baml:21:13 (variable) [declaration] len=1 "n"
// patterns_new.baml:21:16 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:21:23 (variable) len=1 "n"
// patterns_new.baml:22:14 (number) len=1 "0"
// patterns_new.baml:26:1 (comment) len=67 "// Bare type pattern (no binding, no narrow) in match-arm position."
// patterns_new.baml:27:1 (keyword) len=8 "function"
// patterns_new.baml:27:10 (function) [declaration] len=13 "bare_type_arm"
// patterns_new.baml:27:24 (parameter) [declaration] len=1 "v"
// patterns_new.baml:27:27 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:27:31 (operator) len=1 "|"
// patterns_new.baml:27:33 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:27:44 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:28:5 (keyword) len=5 "match"
// patterns_new.baml:28:12 (parameter) len=1 "v"
// patterns_new.baml:29:9 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:29:16 (number) len=1 "1"
// patterns_new.baml:30:9 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:30:19 (number) len=1 "2"
// patterns_new.baml:34:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:35:1 (comment) len=25 "// Function-type patterns"
// patterns_new.baml:36:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:38:1 (comment) len=67 "// Match on a single function type — arm narrows to the callable."
// patterns_new.baml:39:1 (keyword) len=8 "function"
// patterns_new.baml:39:10 (function) [declaration] len=17 "call_int_callback"
// patterns_new.baml:39:28 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:39:33 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:39:41 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:39:49 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:40:5 (keyword) len=5 "match"
// patterns_new.baml:40:12 (parameter) len=2 "cb"
// patterns_new.baml:41:9 (keyword) len=3 "let"
// patterns_new.baml:41:13 (variable) [declaration] len=1 "f"
// patterns_new.baml:41:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:41:25 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:41:29 (keyword) len=6 "throws"
// patterns_new.baml:41:36 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:41:45 (variable) len=1 "f"
// patterns_new.baml:41:47 (number) len=2 "10"
// patterns_new.baml:45:1 (comment) len=74 "// Match on a union of two distinct function types; dispatch by signature."
// patterns_new.baml:46:1 (keyword) len=8 "function"
// patterns_new.baml:46:10 (function) [declaration] len=17 "dispatch_callback"
// patterns_new.baml:46:28 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:46:34 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:46:42 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:46:46 (keyword) len=6 "throws"
// patterns_new.baml:46:53 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:46:60 (operator) len=1 "|"
// patterns_new.baml:46:64 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:46:75 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:46:82 (keyword) len=6 "throws"
// patterns_new.baml:46:89 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:46:100 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:47:5 (keyword) len=5 "match"
// patterns_new.baml:47:12 (parameter) len=2 "cb"
// patterns_new.baml:48:9 (keyword) len=3 "let"
// patterns_new.baml:48:13 (variable) [declaration] len=1 "f"
// patterns_new.baml:48:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:48:25 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:48:29 (keyword) len=6 "throws"
// patterns_new.baml:48:36 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:48:45 (variable) len=1 "f"
// patterns_new.baml:48:47 (number) len=1 "0"
// patterns_new.baml:49:10 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:49:21 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:49:28 (keyword) len=6 "throws"
// patterns_new.baml:49:35 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:49:44 (number) len=1 "0"
// patterns_new.baml:53:1 (comment) len=80 "// Or-pattern across two function-typed alternatives that share the same return."
// patterns_new.baml:54:1 (keyword) len=8 "function"
// patterns_new.baml:54:10 (function) [declaration] len=22 "call_either_int_caller"
// patterns_new.baml:54:33 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:54:39 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:54:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:54:51 (keyword) len=6 "throws"
// patterns_new.baml:54:58 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:54:65 (operator) len=1 "|"
// patterns_new.baml:54:69 (type) [defaultLibrary] len=4 "bool"
// patterns_new.baml:54:78 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:54:82 (keyword) len=6 "throws"
// patterns_new.baml:54:89 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:54:100 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:55:5 (keyword) len=5 "match"
// patterns_new.baml:55:12 (parameter) len=2 "cb"
// patterns_new.baml:56:10 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:56:18 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:56:22 (keyword) len=6 "throws"
// patterns_new.baml:56:29 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:56:38 (number) len=1 "1"
// patterns_new.baml:57:10 (type) [defaultLibrary] len=4 "bool"
// patterns_new.baml:57:19 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:57:23 (keyword) len=6 "throws"
// patterns_new.baml:57:30 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:57:39 (number) len=1 "2"
// patterns_new.baml:61:1 (comment) len=66 "// Function-type narrow with chain-narrow position (`(int) -> T`)."
// patterns_new.baml:62:1 (keyword) len=8 "function"
// patterns_new.baml:62:10 (function) [declaration] len=22 "dispatch_with_wildcard"
// patterns_new.baml:62:33 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:62:39 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:62:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:62:51 (keyword) len=6 "throws"
// patterns_new.baml:62:58 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:62:65 (operator) len=1 "|"
// patterns_new.baml:62:69 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:62:80 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:62:84 (keyword) len=6 "throws"
// patterns_new.baml:62:91 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:62:102 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:63:5 (keyword) len=5 "match"
// patterns_new.baml:63:12 (parameter) len=2 "cb"
// patterns_new.baml:64:10 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:64:18 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:64:22 (keyword) len=6 "throws"
// patterns_new.baml:64:29 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:64:38 (number) len=1 "1"
// patterns_new.baml:65:14 (number) len=1 "0"
// patterns_new.baml:69:1 (comment) len=74 "// Generic function-type pattern. T is unified at the call site. The outer"
// patterns_new.baml:70:1 (comment) len=77 "// parens disambiguate the union — without them the parser binds `|` inside"
// patterns_new.baml:71:1 (comment) len=60 "// the return type, yielding `(int) -> (T | (string) -> T)`."
// patterns_new.baml:72:1 (keyword) len=8 "function"
// patterns_new.baml:72:10 (function) [declaration] len=12 "unwrap_typed"
// patterns_new.baml:72:22 (operator) len=1 "<"
// patterns_new.baml:72:23 (typeParameter) [declaration] len=1 "T"
// patterns_new.baml:72:24 (operator) len=1 ">"
// patterns_new.baml:72:26 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:72:32 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:72:40 (type) len=1 "T"
// patterns_new.baml:72:42 (keyword) len=6 "throws"
// patterns_new.baml:72:49 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:72:56 (operator) len=1 "|"
// patterns_new.baml:72:60 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:72:71 (type) len=1 "T"
// patterns_new.baml:72:73 (keyword) len=6 "throws"
// patterns_new.baml:72:80 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:72:91 (type) len=1 "T"
// patterns_new.baml:73:5 (keyword) len=5 "match"
// patterns_new.baml:73:12 (parameter) len=2 "cb"
// patterns_new.baml:74:9 (keyword) len=3 "let"
// patterns_new.baml:74:13 (variable) [declaration] len=1 "f"
// patterns_new.baml:74:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:74:25 (type) len=1 "T"
// patterns_new.baml:74:27 (keyword) len=6 "throws"
// patterns_new.baml:74:34 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:74:43 (variable) len=1 "f"
// patterns_new.baml:74:45 (number) len=1 "0"
// patterns_new.baml:75:9 (keyword) len=3 "let"
// patterns_new.baml:75:13 (variable) [declaration] len=1 "f"
// patterns_new.baml:75:17 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:75:28 (type) len=1 "T"
// patterns_new.baml:75:30 (keyword) len=6 "throws"
// patterns_new.baml:75:37 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:75:46 (variable) len=1 "f"
// patterns_new.baml:75:48 (string) len=2 "\"\""
// patterns_new.baml:79:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:80:1 (comment) len=74 "// Triple higher-order: function returning a function returning a function"
// patterns_new.baml:81:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:83:1 (comment) len=75 "// `f: (int) -> (int) -> (int) -> int` — three-deep curried int function."
// patterns_new.baml:84:1 (comment) len=78 "// `g: (string) -> (string) -> string`   — two-deep curried string function."
// patterns_new.baml:85:1 (comment) len=73 "// The match dispatches on which curried shape was passed and unwraps it."
// patterns_new.baml:86:1 (keyword) len=8 "function"
// patterns_new.baml:86:10 (function) [declaration] len=14 "unwrap_curried"
// patterns_new.baml:87:5 (parameter) [declaration] len=1 "f"
// patterns_new.baml:87:10 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:87:20 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:87:30 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:87:38 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:87:42 (keyword) len=6 "throws"
// patterns_new.baml:87:49 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:87:56 (keyword) len=6 "throws"
// patterns_new.baml:87:63 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:87:70 (keyword) len=6 "throws"
// patterns_new.baml:87:77 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:87:84 (operator) len=1 "|"
// patterns_new.baml:87:88 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:87:101 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:87:112 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:87:119 (keyword) len=6 "throws"
// patterns_new.baml:87:126 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:87:133 (keyword) len=6 "throws"
// patterns_new.baml:87:140 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:88:6 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:89:5 (keyword) len=5 "match"
// patterns_new.baml:89:12 (parameter) len=1 "f"
// patterns_new.baml:90:9 (keyword) len=3 "let"
// patterns_new.baml:90:13 (variable) [declaration] len=6 "triple"
// patterns_new.baml:90:22 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:90:32 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:90:42 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:90:50 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:90:54 (keyword) len=6 "throws"
// patterns_new.baml:90:61 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:90:68 (keyword) len=6 "throws"
// patterns_new.baml:90:75 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:90:82 (keyword) len=6 "throws"
// patterns_new.baml:90:89 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:90:98 (variable) len=6 "triple"
// patterns_new.baml:90:105 (number) len=1 "1"
// patterns_new.baml:90:108 (number) len=1 "2"
// patterns_new.baml:90:111 (number) len=1 "3"
// patterns_new.baml:91:9 (keyword) len=3 "let"
// patterns_new.baml:91:13 (variable) [declaration] len=4 "pair"
// patterns_new.baml:91:20 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:91:33 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:91:44 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:91:51 (keyword) len=6 "throws"
// patterns_new.baml:91:58 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:91:65 (keyword) len=6 "throws"
// patterns_new.baml:91:72 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:91:81 (number) len=1 "0"
// patterns_new.baml:95:1 (comment) len=68 "// Match on a function that takes a callback. The arm narrows to the"
// patterns_new.baml:96:1 (comment) len=71 "// callback-accepting shape, then we synthesize an int callback inline."
// patterns_new.baml:97:1 (keyword) len=8 "function"
// patterns_new.baml:97:10 (function) [declaration] len=18 "call_with_callback"
// patterns_new.baml:98:5 (parameter) [declaration] len=6 "runner"
// patterns_new.baml:98:16 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:98:24 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:98:28 (keyword) len=6 "throws"
// patterns_new.baml:98:35 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:98:45 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:98:49 (keyword) len=6 "throws"
// patterns_new.baml:98:56 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:98:63 (operator) len=1 "|"
// patterns_new.baml:98:68 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:98:79 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:98:83 (keyword) len=6 "throws"
// patterns_new.baml:98:90 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:98:100 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:98:104 (keyword) len=6 "throws"
// patterns_new.baml:98:111 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:99:6 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:100:5 (keyword) len=5 "match"
// patterns_new.baml:100:12 (parameter) len=6 "runner"
// patterns_new.baml:101:9 (keyword) len=3 "let"
// patterns_new.baml:101:13 (variable) [declaration] len=1 "r"
// patterns_new.baml:101:18 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:101:26 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:101:30 (keyword) len=6 "throws"
// patterns_new.baml:101:37 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:101:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:101:51 (keyword) len=6 "throws"
// patterns_new.baml:101:58 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:101:67 (variable) len=1 "r"
// patterns_new.baml:101:70 (parameter) [declaration] len=1 "x"
// patterns_new.baml:101:73 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:101:81 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:101:87 (parameter) len=1 "x"
// patterns_new.baml:101:89 (operator) len=1 "+"
// patterns_new.baml:101:91 (number) len=1 "1"
// patterns_new.baml:102:9 (keyword) len=3 "let"
// patterns_new.baml:102:13 (variable) [declaration] len=1 "r"
// patterns_new.baml:102:18 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:102:29 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:102:33 (keyword) len=6 "throws"
// patterns_new.baml:102:40 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:102:50 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:102:54 (keyword) len=6 "throws"
// patterns_new.baml:102:61 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:102:70 (variable) len=1 "r"
// patterns_new.baml:102:73 (parameter) [declaration] len=1 "s"
// patterns_new.baml:102:76 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:102:87 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:102:93 (number) len=1 "0"
// patterns_new.baml:106:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:107:1 (comment) len=77 "// Chain narrows + subtyping: each link must be a subtype of the next, so the"
// patterns_new.baml:108:1 (comment) len=77 "// chain widens left-to-right. The rightmost concrete narrow is the effective"
// patterns_new.baml:109:1 (comment) len=14 "// annotation."
// patterns_new.baml:110:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:112:1 (comment) len=52 "// Literal-narrow widening to primitive: `1 <: int`."
// patterns_new.baml:113:1 (keyword) len=8 "function"
// patterns_new.baml:113:10 (function) [declaration] len=21 "literal_widens_to_int"
// patterns_new.baml:113:32 (parameter) [declaration] len=1 "v"
// patterns_new.baml:113:35 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:113:43 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:114:5 (keyword) len=5 "match"
// patterns_new.baml:114:12 (parameter) len=1 "v"
// patterns_new.baml:115:9 (keyword) len=3 "let"
// patterns_new.baml:115:13 (variable) [declaration] len=1 "n"
// patterns_new.baml:115:16 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:115:23 (variable) len=1 "n"
// patterns_new.baml:119:1 (comment) len=78 "// Function-type widening: a narrower return type is a subtype of a wider one,"
// patterns_new.baml:120:1 (comment) len=46 "// so `(int) -> int <: (int) -> int | string`."
// patterns_new.baml:121:1 (keyword) len=8 "function"
// patterns_new.baml:121:10 (function) [declaration] len=16 "fn_return_widens"
// patterns_new.baml:121:27 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:121:32 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:121:40 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:121:44 (operator) len=1 "|"
// patterns_new.baml:121:46 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:121:57 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:122:5 (keyword) len=5 "match"
// patterns_new.baml:122:12 (parameter) len=2 "cb"
// patterns_new.baml:123:10 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:123:19 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:123:23 (operator) len=1 "|"
// patterns_new.baml:123:25 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:123:33 (keyword) len=6 "throws"
// patterns_new.baml:123:40 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:123:49 (number) len=1 "1"
// patterns_new.baml:127:1 (keyword) len=4 "type"
// patterns_new.baml:127:6 (type) [declaration] len=6 "Number"
// patterns_new.baml:127:13 (operator) len=1 "="
// patterns_new.baml:127:15 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:127:19 (operator) len=1 "|"
// patterns_new.baml:127:21 (type) [defaultLibrary] len=5 "float"
// patterns_new.baml:128:1 (keyword) len=8 "function"
// patterns_new.baml:128:10 (function) [declaration] len=19 "int_widens_to_alias"
// patterns_new.baml:128:30 (parameter) [declaration] len=1 "v"
// patterns_new.baml:128:33 (type) len=6 "Number"
// patterns_new.baml:128:44 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:129:5 (keyword) len=5 "match"
// patterns_new.baml:129:12 (parameter) len=1 "v"
// patterns_new.baml:130:9 (keyword) len=3 "let"
// patterns_new.baml:130:13 (variable) [declaration] len=1 "n"
// patterns_new.baml:130:16 (type) len=6 "Number"
// patterns_new.baml:130:26 (number) len=1 "1"
// patterns_new.baml:134:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:135:1 (comment) len=42 "// Nested unions and chains in parentheses"
// patterns_new.baml:136:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:138:1 (comment) len=52 "// Paren-wrapped Or with each branch itself a chain."
// patterns_new.baml:139:1 (keyword) len=8 "function"
// patterns_new.baml:139:10 (function) [declaration] len=18 "paren_chains_in_or"
// patterns_new.baml:139:29 (parameter) [declaration] len=1 "v"
// patterns_new.baml:139:32 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:139:40 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:140:5 (keyword) len=5 "match"
// patterns_new.baml:140:12 (parameter) len=1 "v"
// patterns_new.baml:141:11 (keyword) len=3 "let"
// patterns_new.baml:141:15 (variable) [declaration] len=1 "x"
// patterns_new.baml:141:18 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:141:24 (operator) len=1 "|"
// patterns_new.baml:141:28 (keyword) len=3 "let"
// patterns_new.baml:141:32 (variable) [declaration] len=1 "x"
// patterns_new.baml:141:35 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:141:44 (number) len=1 "1"
// patterns_new.baml:145:1 (keyword) len=8 "function"
// patterns_new.baml:145:10 (function) [declaration] len=10 "fn_pattern"
// patterns_new.baml:145:21 (parameter) [declaration] len=2 "cb"
// patterns_new.baml:145:26 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:145:34 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:145:42 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:146:5 (keyword) len=5 "match"
// patterns_new.baml:146:12 (parameter) len=2 "cb"
// patterns_new.baml:147:9 (keyword) len=3 "let"
// patterns_new.baml:147:13 (variable) [declaration] len=1 "f"
// patterns_new.baml:147:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:147:25 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:147:29 (keyword) len=6 "throws"
// patterns_new.baml:147:36 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:147:45 (variable) len=1 "f"
// patterns_new.baml:147:47 (number) len=1 "0"
// patterns_new.baml:151:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:152:1 (comment) len=28 "// for-let with crazy chains"
// patterns_new.baml:153:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:155:1 (comment) len=37 "// for-let with a single type narrow."
// patterns_new.baml:156:1 (keyword) len=8 "function"
// patterns_new.baml:156:10 (function) [declaration] len=19 "for_let_with_narrow"
// patterns_new.baml:156:30 (parameter) [declaration] len=5 "items"
// patterns_new.baml:156:37 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:156:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:157:5 (keyword) len=3 "let"
// patterns_new.baml:157:9 (variable) [declaration] len=3 "sum"
// patterns_new.baml:157:13 (operator) len=1 "="
// patterns_new.baml:157:15 (number) len=1 "0"
// patterns_new.baml:158:5 (keyword) len=3 "for"
// patterns_new.baml:158:10 (keyword) len=3 "let"
// patterns_new.baml:158:14 (variable) [declaration] len=1 "n"
// patterns_new.baml:158:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:158:21 (keyword) len=2 "in"
// patterns_new.baml:158:24 (parameter) len=5 "items"
// patterns_new.baml:159:9 (variable) len=3 "sum"
// patterns_new.baml:159:13 (operator) len=2 "+="
// patterns_new.baml:159:16 (variable) len=1 "n"
// patterns_new.baml:161:5 (variable) len=3 "sum"
// patterns_new.baml:164:1 (keyword) len=8 "function"
// patterns_new.baml:164:10 (function) [declaration] len=18 "for_let_ascription"
// patterns_new.baml:164:29 (parameter) [declaration] len=5 "items"
// patterns_new.baml:164:36 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:164:46 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:165:5 (keyword) len=3 "let"
// patterns_new.baml:165:9 (variable) [declaration] len=3 "sum"
// patterns_new.baml:165:13 (operator) len=1 "="
// patterns_new.baml:165:15 (number) len=1 "0"
// patterns_new.baml:166:5 (keyword) len=3 "for"
// patterns_new.baml:166:10 (keyword) len=3 "let"
// patterns_new.baml:166:14 (variable) [declaration] len=1 "n"
// patterns_new.baml:166:17 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:166:21 (operator) len=1 "|"
// patterns_new.baml:166:23 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:166:30 (keyword) len=2 "in"
// patterns_new.baml:166:33 (parameter) len=5 "items"
// patterns_new.baml:167:9 (variable) len=3 "sum"
// patterns_new.baml:167:13 (operator) len=2 "+="
// patterns_new.baml:167:16 (number) len=1 "1"
// patterns_new.baml:169:5 (variable) len=3 "sum"
// patterns_new.baml:172:1 (comment) len=39 "// for-let with a function-type narrow."
// patterns_new.baml:173:1 (keyword) len=8 "function"
// patterns_new.baml:173:10 (function) [declaration] len=17 "for_let_fn_narrow"
// patterns_new.baml:173:28 (parameter) [declaration] len=5 "items"
// patterns_new.baml:173:37 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:173:45 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:173:49 (keyword) len=6 "throws"
// patterns_new.baml:173:56 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:173:69 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:174:5 (keyword) len=3 "let"
// patterns_new.baml:174:9 (variable) [declaration] len=5 "total"
// patterns_new.baml:174:15 (operator) len=1 "="
// patterns_new.baml:174:17 (number) len=1 "0"
// patterns_new.baml:175:5 (keyword) len=3 "for"
// patterns_new.baml:175:10 (keyword) len=3 "let"
// patterns_new.baml:175:14 (variable) [declaration] len=1 "f"
// patterns_new.baml:175:18 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:175:26 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:175:30 (keyword) len=6 "throws"
// patterns_new.baml:175:37 (type) [defaultLibrary] len=5 "never"
// patterns_new.baml:175:43 (keyword) len=2 "in"
// patterns_new.baml:175:46 (parameter) len=5 "items"
// patterns_new.baml:176:9 (variable) len=5 "total"
// patterns_new.baml:176:15 (operator) len=2 "+="
// patterns_new.baml:176:18 (variable) len=1 "f"
// patterns_new.baml:176:20 (number) len=1 "0"
// patterns_new.baml:178:5 (variable) len=5 "total"
// patterns_new.baml:181:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:182:1 (comment) len=31 "// catch arms with crazy chains"
// patterns_new.baml:183:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:185:1 (keyword) len=5 "class"
// patterns_new.baml:185:7 (class) [declaration] len=8 "AppError"
// patterns_new.baml:185:18 (property) [declaration] len=4 "code"
// patterns_new.baml:185:23 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:187:1 (keyword) len=8 "function"
// patterns_new.baml:187:10 (function) [declaration] len=11 "risky_error"
// patterns_new.baml:187:22 (parameter) [declaration] len=4 "mode"
// patterns_new.baml:187:28 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:187:36 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:187:40 (keyword) len=6 "throws"
// patterns_new.baml:187:47 (class) len=8 "AppError"
// patterns_new.baml:187:56 (operator) len=1 "|"
// patterns_new.baml:187:58 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:188:5 (keyword) len=2 "if"
// patterns_new.baml:188:9 (parameter) len=4 "mode"
// patterns_new.baml:188:14 (operator) len=2 "=="
// patterns_new.baml:188:17 (number) len=1 "0"
// patterns_new.baml:189:9 (keyword) len=5 "throw"
// patterns_new.baml:189:15 (class) len=8 "AppError"
// patterns_new.baml:189:26 (property) len=4 "code"
// patterns_new.baml:189:32 (number) len=1 "1"
// patterns_new.baml:191:5 (keyword) len=5 "throw"
// patterns_new.baml:191:11 (string) len=6 "\"boom\""
// patterns_new.baml:194:1 (comment) len=70 "// Catch arm with a chain narrow of a union — binds the joined type."
// patterns_new.baml:195:1 (keyword) len=8 "function"
// patterns_new.baml:195:10 (function) [declaration] len=17 "catch_chain_union"
// patterns_new.baml:195:28 (parameter) [declaration] len=4 "mode"
// patterns_new.baml:195:34 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:195:42 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:196:5 (function) len=11 "risky_error"
// patterns_new.baml:196:17 (parameter) len=4 "mode"
// patterns_new.baml:196:23 (keyword) len=5 "catch"
// patterns_new.baml:196:30 (parameter) [declaration] len=1 "e"
// patterns_new.baml:197:9 (class) len=8 "AppError"
// patterns_new.baml:197:18 (operator) len=1 "|"
// patterns_new.baml:197:20 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:197:30 (number) len=1 "1"
// patterns_new.baml:201:1 (comment) len=72 "// Catch arms with paren-wrapped Or chains, each branch a typed binding."
// patterns_new.baml:202:1 (keyword) len=8 "function"
// patterns_new.baml:202:10 (function) [declaration] len=20 "catch_paren_or_chain"
// patterns_new.baml:202:31 (parameter) [declaration] len=4 "mode"
// patterns_new.baml:202:37 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:202:45 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:203:5 (function) len=11 "risky_error"
// patterns_new.baml:203:17 (parameter) len=4 "mode"
// patterns_new.baml:203:23 (keyword) len=5 "catch"
// patterns_new.baml:203:30 (parameter) [declaration] len=1 "e"
// patterns_new.baml:204:10 (class) len=8 "AppError"
// patterns_new.baml:204:20 (operator) len=1 "|"
// patterns_new.baml:204:23 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:204:34 (number) len=1 "1"
// patterns_new.baml:208:1 (comment) len=74 "// Catch arms with widening chain narrows (literal -> primitive -> union)."
// patterns_new.baml:209:1 (keyword) len=8 "function"
// patterns_new.baml:209:10 (function) [declaration] len=20 "catch_widening_chain"
// patterns_new.baml:209:31 (parameter) [declaration] len=4 "mode"
// patterns_new.baml:209:37 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:209:45 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:210:5 (function) len=11 "risky_error"
// patterns_new.baml:210:17 (parameter) len=4 "mode"
// patterns_new.baml:210:23 (keyword) len=5 "catch"
// patterns_new.baml:210:30 (parameter) [declaration] len=1 "e"
// patterns_new.baml:211:9 (keyword) len=3 "let"
// patterns_new.baml:211:13 (variable) [declaration] len=1 "s"
// patterns_new.baml:211:16 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:211:26 (number) len=1 "1"
// patterns_new.baml:212:9 (keyword) len=3 "let"
// patterns_new.baml:212:13 (variable) [declaration] len=3 "app"
// patterns_new.baml:212:18 (class) len=8 "AppError"
// patterns_new.baml:212:30 (number) len=1 "2"
// patterns_new.baml:216:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:217:1 (comment) len=61 "// Type unions with suffixes wrapped in many layers of parens"
// patterns_new.baml:218:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:220:1 (comment) len=55 "// Single-layer paren around a union with array suffix."
// patterns_new.baml:221:1 (keyword) len=8 "function"
// patterns_new.baml:221:10 (function) [declaration] len=28 "nested_paren_union_array_one"
// patterns_new.baml:221:39 (parameter) [declaration] len=5 "items"
// patterns_new.baml:221:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:221:51 (operator) len=1 "|"
// patterns_new.baml:221:53 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:221:67 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:222:5 (parameter) len=5 "items"
// patterns_new.baml:222:11 (method) len=6 "length"
// patterns_new.baml:225:1 (comment) len=76 "// Two paren layers: `((int | string)[])` is the same as `(int | string)[]`."
// patterns_new.baml:226:1 (keyword) len=8 "function"
// patterns_new.baml:226:10 (function) [declaration] len=28 "nested_paren_union_array_two"
// patterns_new.baml:226:39 (parameter) [declaration] len=5 "items"
// patterns_new.baml:226:48 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:226:52 (operator) len=1 "|"
// patterns_new.baml:226:54 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:226:69 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:227:5 (parameter) len=5 "items"
// patterns_new.baml:227:11 (method) len=6 "length"
// patterns_new.baml:230:1 (comment) len=51 "// Three paren layers. Chain narrow on the outside."
// patterns_new.baml:231:1 (keyword) len=8 "function"
// patterns_new.baml:231:10 (function) [declaration] len=30 "nested_paren_union_array_three"
// patterns_new.baml:231:41 (parameter) [declaration] len=1 "v"
// patterns_new.baml:231:47 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:231:51 (operator) len=1 "|"
// patterns_new.baml:231:53 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:231:71 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:232:5 (parameter) len=1 "v"
// patterns_new.baml:232:7 (method) len=6 "length"
// patterns_new.baml:235:1 (comment) len=61 "// Chained suffix: union with `[]` wrapped, then `?` outside."
// patterns_new.baml:236:1 (keyword) len=8 "function"
// patterns_new.baml:236:10 (function) [declaration] len=33 "nested_paren_union_array_optional"
// patterns_new.baml:236:44 (parameter) [declaration] len=1 "v"
// patterns_new.baml:236:49 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:236:53 (operator) len=1 "|"
// patterns_new.baml:236:55 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:236:71 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:237:5 (keyword) len=5 "match"
// patterns_new.baml:237:12 (parameter) len=1 "v"
// patterns_new.baml:238:11 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:238:15 (operator) len=1 "|"
// patterns_new.baml:238:17 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:238:32 (number) len=1 "1"
// patterns_new.baml:242:1 (comment) len=66 "// In a match arm, deeply parenthesized type union as the pattern."
// patterns_new.baml:243:1 (keyword) len=8 "function"
// patterns_new.baml:243:10 (function) [declaration] len=24 "match_nested_paren_union"
// patterns_new.baml:243:35 (parameter) [declaration] len=1 "v"
// patterns_new.baml:243:39 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:243:43 (operator) len=1 "|"
// patterns_new.baml:243:45 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:243:59 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:244:5 (keyword) len=5 "match"
// patterns_new.baml:244:12 (parameter) len=1 "v"
// patterns_new.baml:245:12 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:245:16 (operator) len=1 "|"
// patterns_new.baml:245:18 (type) [defaultLibrary] len=6 "string"
// patterns_new.baml:245:33 (number) len=1 "1"
// patterns_new.baml:249:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:250:1 (comment) len=62 "// Nested array/class destructuring with branch-local bindings"
// patterns_new.baml:251:1 (comment) len=79 "// ============================================================================"
// patterns_new.baml:253:1 (keyword) len=5 "class"
// patterns_new.baml:253:7 (class) [declaration] len=13 "PatternBucket"
// patterns_new.baml:254:5 (property) [declaration] len=6 "values"
// patterns_new.baml:254:12 (type) [defaultLibrary] len=3 "int"
// patterns_new.baml:257:1 (keyword) len=5 "class"
// patterns_new.baml:257:7 (class) [declaration] len=13 "PatternMatrix"
// patterns_new.baml:258:5 (property) [declaration] len=4 "rows"
// patterns_new.baml:258:10 (type) [defaultLibrary] len=3 "int"
