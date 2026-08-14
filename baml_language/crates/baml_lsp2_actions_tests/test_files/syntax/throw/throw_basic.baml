// Basic throw statement — no diagnostics expected
function ThrowString() -> string {
  throw "something went wrong"
  "unreachable"
}

// Throw integer value
function ThrowInt() -> int {
  throw 42
  0
}

// Throw variable
function ThrowVariable(msg: string) -> string {
  throw msg
  "unreachable"
}

//----
//- diagnostics
// E0146
//
//   ⚠ unreachable code: 1 statement(s) after diverging statement
//    ╭─[throw_throw_basic.baml:3:3]
//  3 │   throw "something went wrong"
//    ·   ────────────────────────────
//    ╰────
// E0146
//
//   ⚠ unreachable code: 1 statement(s) after diverging statement
//    ╭─[throw_throw_basic.baml:9:3]
//  9 │   throw 42
//    ·   ────────
//    ╰────
// E0146
//
//   ⚠ unreachable code: 1 statement(s) after diverging statement
//     ╭─[throw_throw_basic.baml:15:3]
//  15 │   throw msg
//     ·   ─────────
//     ╰────
