// BEP-007: Basic throws syntax on function declarations

class MyError {
  message string
}

// Function with throws clause
function MayThrow(x: int) -> string throws MyError {
  if (x < 0) {
    throw MyError { message: "negative" }
  }
  "ok"
}

// Function with throws never (does not throw)
function NoThrow(x: int) -> string {
  "always ok"
}

// Function with union throws clause
function MayThrowMultiple(x: int) -> string throws string | int {
  if (x < 0) {
    throw "negative"
  }
  "ok"
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: int
//     ╭─[throws_clause.baml:21:65]
//  21 │ ╭─▶ function MayThrowMultiple(x: int) -> string throws string | int {
//  22 │ │     if (x < 0) {
//  23 │ │       throw "negative"
//  24 │ │     }
//  25 │ │     "ok"
//  26 │ ╰─▶ }
//     ╰────
//
//- semantic_tokens
// throws_clause.baml:1:1 (comment) len=56 "// BEP-007: Basic throws syntax on function declarations"
// throws_clause.baml:3:1 (keyword) len=5 "class"
// throws_clause.baml:3:7 (class) [declaration] len=7 "MyError"
// throws_clause.baml:4:3 (property) [declaration] len=7 "message"
// throws_clause.baml:4:11 (type) [defaultLibrary] len=6 "string"
// throws_clause.baml:7:1 (comment) len=30 "// Function with throws clause"
// throws_clause.baml:8:1 (keyword) len=8 "function"
// throws_clause.baml:8:10 (function) [declaration] len=8 "MayThrow"
// throws_clause.baml:8:19 (parameter) [declaration] len=1 "x"
// throws_clause.baml:8:22 (type) [defaultLibrary] len=3 "int"
// throws_clause.baml:8:30 (type) [defaultLibrary] len=6 "string"
// throws_clause.baml:8:37 (keyword) len=6 "throws"
// throws_clause.baml:8:44 (class) len=7 "MyError"
// throws_clause.baml:9:3 (keyword) len=2 "if"
// throws_clause.baml:9:7 (parameter) len=1 "x"
// throws_clause.baml:9:9 (operator) len=1 "<"
// throws_clause.baml:9:11 (number) len=1 "0"
// throws_clause.baml:10:5 (keyword) len=5 "throw"
// throws_clause.baml:10:11 (class) len=7 "MyError"
// throws_clause.baml:10:21 (property) len=7 "message"
// throws_clause.baml:10:30 (string) len=10 "\"negative\""
// throws_clause.baml:12:3 (string) len=4 "\"ok\""
// throws_clause.baml:15:1 (comment) len=46 "// Function with throws never (does not throw)"
// throws_clause.baml:16:1 (keyword) len=8 "function"
// throws_clause.baml:16:10 (function) [declaration] len=7 "NoThrow"
// throws_clause.baml:16:18 (parameter) [declaration] len=1 "x"
// throws_clause.baml:16:21 (type) [defaultLibrary] len=3 "int"
// throws_clause.baml:16:29 (type) [defaultLibrary] len=6 "string"
// throws_clause.baml:17:3 (string) len=11 "\"always ok\""
// throws_clause.baml:20:1 (comment) len=36 "// Function with union throws clause"
// throws_clause.baml:21:1 (keyword) len=8 "function"
// throws_clause.baml:21:10 (function) [declaration] len=16 "MayThrowMultiple"
// throws_clause.baml:21:27 (parameter) [declaration] len=1 "x"
// throws_clause.baml:21:30 (type) [defaultLibrary] len=3 "int"
// throws_clause.baml:21:38 (type) [defaultLibrary] len=6 "string"
// throws_clause.baml:21:45 (keyword) len=6 "throws"
// throws_clause.baml:21:52 (type) [defaultLibrary] len=6 "string"
// throws_clause.baml:21:59 (operator) len=1 "|"
// throws_clause.baml:21:61 (type) [defaultLibrary] len=3 "int"
// throws_clause.baml:22:3 (keyword) len=2 "if"
// throws_clause.baml:22:7 (parameter) len=1 "x"
// throws_clause.baml:22:9 (operator) len=1 "<"
// throws_clause.baml:22:11 (number) len=1 "0"
// throws_clause.baml:23:5 (keyword) len=5 "throw"
// throws_clause.baml:23:11 (string) len=10 "\"negative\""
// throws_clause.baml:25:3 (string) len=4 "\"ok\""
