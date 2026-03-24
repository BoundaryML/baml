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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable code: 1 statement(s) after diverging statement
//    ╭─[ throw_throw_basic.baml:3:2 ]
//    │
//  3 │ ╭─▶   throw "something went wrong"
//  4 │ ├─▶   "unreachable"
//    │ │                     
//    │ ╰───────────────────── unreachable code: 1 statement(s) after diverging statement
//    │     
//    │     Note: Error code: E0001
// ───╯
// Error: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ throw_throw_basic.baml:9:2 ]
//     │
//   9 │ ╭─▶   throw 42
//  10 │ ├─▶   0
//     │ │         
//     │ ╰───────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable code: 1 statement(s) after diverging statement
//     ╭─[ throw_throw_basic.baml:15:2 ]
//     │
//  15 │ ╭─▶   throw msg
//  16 │ ├─▶   "unreachable"
//     │ │                     
//     │ ╰───────────────────── unreachable code: 1 statement(s) after diverging statement
//     │     
//     │     Note: Error code: E0001
// ────╯
