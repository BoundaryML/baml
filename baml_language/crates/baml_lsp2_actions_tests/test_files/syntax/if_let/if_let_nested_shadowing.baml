// Nested if-let: inner binding shadows the outer one. After the inner
// if-let, the outer binding must be back in scope. After both if-lets,
// neither binding may be referenced.
class Wrapper {
  payload string
}
class Empty {}

function Shadowing(outer: Wrapper | Empty, inner: Wrapper | Empty) -> string {
  if let w: Wrapper = outer {
    if let w: Wrapper = inner {
      // `w` here is the inner binding.
      w.payload
    } else {
      // Inner scope ended; outer `w` is back.
      w.payload
    }
  } else {
    "none"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
