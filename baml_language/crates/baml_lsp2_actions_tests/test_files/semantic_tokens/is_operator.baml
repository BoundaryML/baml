// `<expr> is <pattern>` — Rust `matches!`-style pattern test.
// Returns true if the scrutinee matches the pattern, false otherwise.

// Simple type check on a union.
function is_int(v: int | string) -> bool {
    v is int
}

// Negation by composing with `!`.
function is_not_string(v: int | string) -> bool {
    !(v is string)
}

// Or-pattern on the RHS.
function is_int_or_bool(v: int | string | bool) -> bool {
    v is int | bool
}

// Use inside an `if` condition.
function classify(v: int | string) -> string {
    if (v is int) { "number" }
    else { "text" }
}

// Chain into a boolean expression with `&&`.
function both_ints(a: int | string, b: int | string) -> bool {
    a is int && b is int
}

// Literal pattern.
function is_zero(n: int) -> bool {
    n is 0
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// is_operator.baml:1:1 (comment) len=64 "// `<expr> is <pattern>` — Rust `matches!`-style pattern test."
// is_operator.baml:2:1 (comment) len=70 "// Returns true if the scrutinee matches the pattern, false otherwise."
// is_operator.baml:4:1 (comment) len=32 "// Simple type check on a union."
// is_operator.baml:5:1 (keyword) len=8 "function"
// is_operator.baml:5:10 (function) [declaration] len=6 "is_int"
// is_operator.baml:5:17 (parameter) [declaration] len=1 "v"
// is_operator.baml:5:20 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:5:24 (operator) len=1 "|"
// is_operator.baml:5:26 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:5:37 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:6:5 (parameter) len=1 "v"
// is_operator.baml:6:7 (keyword) len=2 "is"
// is_operator.baml:6:10 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:9:1 (comment) len=34 "// Negation by composing with `!`."
// is_operator.baml:10:1 (keyword) len=8 "function"
// is_operator.baml:10:10 (function) [declaration] len=13 "is_not_string"
// is_operator.baml:10:24 (parameter) [declaration] len=1 "v"
// is_operator.baml:10:27 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:10:31 (operator) len=1 "|"
// is_operator.baml:10:33 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:10:44 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:11:5 (operator) len=1 "!"
// is_operator.baml:11:7 (parameter) len=1 "v"
// is_operator.baml:11:9 (keyword) len=2 "is"
// is_operator.baml:11:12 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:14:1 (comment) len=25 "// Or-pattern on the RHS."
// is_operator.baml:15:1 (keyword) len=8 "function"
// is_operator.baml:15:10 (function) [declaration] len=14 "is_int_or_bool"
// is_operator.baml:15:25 (parameter) [declaration] len=1 "v"
// is_operator.baml:15:28 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:15:32 (operator) len=1 "|"
// is_operator.baml:15:34 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:15:41 (operator) len=1 "|"
// is_operator.baml:15:43 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:15:52 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:16:5 (parameter) len=1 "v"
// is_operator.baml:16:7 (keyword) len=2 "is"
// is_operator.baml:16:10 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:16:14 (operator) len=1 "|"
// is_operator.baml:16:16 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:19:1 (comment) len=32 "// Use inside an `if` condition."
// is_operator.baml:20:1 (keyword) len=8 "function"
// is_operator.baml:20:10 (function) [declaration] len=8 "classify"
// is_operator.baml:20:19 (parameter) [declaration] len=1 "v"
// is_operator.baml:20:22 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:20:26 (operator) len=1 "|"
// is_operator.baml:20:28 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:20:39 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:21:5 (keyword) len=2 "if"
// is_operator.baml:21:9 (parameter) len=1 "v"
// is_operator.baml:21:11 (keyword) len=2 "is"
// is_operator.baml:21:14 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:21:21 (string) len=8 "\"number\""
// is_operator.baml:22:5 (keyword) len=4 "else"
// is_operator.baml:22:12 (string) len=6 "\"text\""
// is_operator.baml:25:1 (comment) len=45 "// Chain into a boolean expression with `&&`."
// is_operator.baml:26:1 (keyword) len=8 "function"
// is_operator.baml:26:10 (function) [declaration] len=9 "both_ints"
// is_operator.baml:26:20 (parameter) [declaration] len=1 "a"
// is_operator.baml:26:23 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:26:27 (operator) len=1 "|"
// is_operator.baml:26:29 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:26:37 (parameter) [declaration] len=1 "b"
// is_operator.baml:26:40 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:26:44 (operator) len=1 "|"
// is_operator.baml:26:46 (type) [defaultLibrary] len=6 "string"
// is_operator.baml:26:57 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:27:5 (parameter) len=1 "a"
// is_operator.baml:27:7 (keyword) len=2 "is"
// is_operator.baml:27:10 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:27:14 (operator) len=2 "&&"
// is_operator.baml:27:17 (parameter) len=1 "b"
// is_operator.baml:27:19 (keyword) len=2 "is"
// is_operator.baml:27:22 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:30:1 (comment) len=19 "// Literal pattern."
// is_operator.baml:31:1 (keyword) len=8 "function"
// is_operator.baml:31:10 (function) [declaration] len=7 "is_zero"
// is_operator.baml:31:18 (parameter) [declaration] len=1 "n"
// is_operator.baml:31:21 (type) [defaultLibrary] len=3 "int"
// is_operator.baml:31:29 (type) [defaultLibrary] len=4 "bool"
// is_operator.baml:32:5 (parameter) len=1 "n"
// is_operator.baml:32:7 (keyword) len=2 "is"
// is_operator.baml:32:10 (number) len=1 "0"
