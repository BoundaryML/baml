// Two let-else statements back-to-back: both bindings reach the tail and
// each name lives in the enclosing scope independently.
class Ok { value string }
class Err { message string }

function ChainBoth(a: Ok | Err, b: Ok | Err) -> string {
  let oa: Ok = a else { return "a-failed"; };
  let ob: Ok = b else { return "b-failed"; };
  oa.value + "-" + ob.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
