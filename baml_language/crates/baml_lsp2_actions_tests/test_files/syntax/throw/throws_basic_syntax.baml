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
// warning: extraneous throws declaration: int
//     ╭─[ throw_throws_basic_syntax.baml:21:51 ]
//     │
//  21 │ function MayThrowMultiple(x: int) -> string throws string | int {
//     │                                                   ──────┬──────  
//     │                                                         ╰──────── extraneous throws declaration: int
//     │ 
//     │ Note: Error code: E0097
// ────╯
