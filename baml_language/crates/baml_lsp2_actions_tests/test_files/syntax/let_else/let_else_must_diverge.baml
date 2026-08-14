// The else block must diverge (`!`). Returning a value is an error (E0113).
class Ok { value string }
class Err { message string }

function NotDiverging(r: Ok | Err) -> string {
  let o: Ok = r else { "fallback" };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
