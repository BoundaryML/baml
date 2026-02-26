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
// <no-diagnostics-expected>
