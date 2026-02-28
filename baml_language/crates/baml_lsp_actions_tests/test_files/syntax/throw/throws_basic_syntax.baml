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
// Warning: `throws` declaration includes types the function never throws: int
//     ╭─[ throw_throws_basic_syntax.baml:21:37 ]
//     │
//  21 │ function MayThrowMultiple(x: int) -> string throws string | int {
//     │                                     ───┬───  
//     │                                        ╰───── `throws` declaration includes types the function never throws: int
//     │ 
//     │ Note: Error code: E0097
// ────╯
