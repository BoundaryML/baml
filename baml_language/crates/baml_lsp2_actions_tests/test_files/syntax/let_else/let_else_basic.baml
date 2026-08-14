// `let … else` with a refutable type pattern. Bindings flow into the
// enclosing scope on a successful match; on failure the else block must
// diverge (here, `return`).
class Ok {
  value string
}
class Err {
  message string
}

function get_value(r: Ok | Err) -> string {
  let o: Ok = r else { return "no match"; };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
