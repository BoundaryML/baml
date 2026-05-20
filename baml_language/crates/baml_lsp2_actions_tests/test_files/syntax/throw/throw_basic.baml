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
// warning: unreachable code: 1 statement(s) after diverging statement
//    ╭─[ throw_throw_basic.baml:2:35 ]
//    │
//  2 │ ╭─▶ function ThrowString() -> string {
//  3 │ ├─▶   throw "something went wrong"
//    │ │                                    
//    │ ╰──────────────────────────────────── unreachable code: 1 statement(s) after diverging statement
//    │     
//    │     Note: Error code: E0001
// ───╯
// warning: unreachable code: 1 statement(s) after diverging statement
//    ╭─[ throw_throw_basic.baml:8:29 ]
//    │
//  8 │ ╭─▶ function ThrowInt() -> int {
//  9 │ ├─▶   throw 42
//    │ │                
//    │ ╰──────────────── unreachable code: 1 statement(s) after diverging statement
//    │     
//    │     Note: Error code: E0001
// ───╯
// warning: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ throw_throw_basic.baml:14:48 ]
//     │
//  14 │ ╭─▶ function ThrowVariable(msg: string) -> string {
//  15 │ ├─▶   throw msg
//     │ │                 
//     │ ╰───────────────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
