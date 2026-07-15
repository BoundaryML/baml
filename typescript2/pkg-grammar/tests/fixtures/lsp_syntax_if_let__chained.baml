// `else if let` and `else if` chains.
class Ok {
  value string
}
class Err {
  message string
}
class Empty {}

// Plain `else if` after `if let`. The `is Empty { ... }` form is no
// longer ambiguous with the then-block in condition position — the
// parser suppresses destructure patterns there (Rust-style restriction).
function MixedChainPlain(r: Ok | Err | Empty) -> string {
  if let o: Ok = r {
    o.value
  } else if r is Empty {
    "empty"
  } else {
    "other"
  }
}

// `else if let` chain — each branch binds its own name.
function IfLetChain(r: Ok | Err | Empty) -> string {
  if let o: Ok = r {
    o.value
  } else if let e: Err = r {
    e.message
  } else {
    "empty"
  }
}

// `else if let` without a final else.
function IfLetChainNoElse(r: Ok | Err) -> string {
  if let o: Ok = r {
    return o.value;
  } else if let e: Err = r {
    return e.message;
  }
  "unreachable"
}
