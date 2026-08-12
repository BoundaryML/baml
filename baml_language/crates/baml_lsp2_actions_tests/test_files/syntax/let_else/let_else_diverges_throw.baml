// `throw` in the else branch is also an accepted diverging form. The thrown
// type flows into the function's declared throws contract.
class Ok { value string }
class Err { message string }
class NoMatch {}

function DivergeByThrow(r: Ok | Err) -> string throws NoMatch {
  let o: Ok = r else { throw NoMatch {}; };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
