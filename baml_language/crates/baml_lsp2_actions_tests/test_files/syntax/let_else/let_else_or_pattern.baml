// Or-pattern in let-else: both alternatives bind `s` to a class with a
// `value` field. The binding's type is `Ok | Warn`.
class Ok { value string }
class Warn { value string }
class Err { message string }

function OrPatternBindsCommon(r: Ok | Warn | Err) -> string {
  let s: Ok | Warn = r else { return "err"; };
  s.value
}

//----
//- diagnostics
// <no-diagnostics-expected>
