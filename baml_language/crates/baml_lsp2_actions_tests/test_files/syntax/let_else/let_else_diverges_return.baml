// `return` in the else branch is one accepted diverging form.
class Ok { value string }
class Err { message string }

function Acceptable(r: Ok | Err) -> string {
  let o: Ok = r else { return "fallback"; };
  o.value
}

// A call whose declared return type is `never` also diverges, even though
// call inference does not set the statement-level divergence flag itself.
function PanicOnMiss(r: Ok | Err) -> string {
  let o: Ok = r else { baml.sys.panic("no match") };
  o.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
