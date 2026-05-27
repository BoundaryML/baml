// Basic if-let: simple binding plus narrowing on the scrutinee.
class Ok {
  value string
}
class Err {
  message string
}

// Bind the scrutinee under a refined type.
function RefineSimple(r: Ok | Err) -> string {
  if let o: Ok = r {
    o.value
  } else {
    "fallback"
  }
}

// Without an else, the if-let returns void; here we use a statement form.
function StatementForm(r: Ok | Err) -> string {
  if let o: Ok = r {
    return o.value;
  }
  "no-match"
}

// Pattern bindings only visible in the then-branch.
function ElseSeesResidual(r: Ok | Err) -> string {
  if let o: Ok = r {
    o.value
  } else {
    r.message
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
