// `return` in the else branch is one accepted diverging form.
class Ok { value string }
class Err { message string }

function Acceptable(r: Ok | Err) -> string {
  let o: Ok = r else { return "fallback"; };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
