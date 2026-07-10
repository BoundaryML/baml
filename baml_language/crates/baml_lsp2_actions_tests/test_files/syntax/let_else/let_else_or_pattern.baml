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
// error: type `Ok | Warn` has no member `value`: its members implement no common interface that declares `value`
//    ╭─[ let_else_let_else_or_pattern.baml:9:5 ]
//    │
//  9 │   s.value
//    │     ──┬──
//    │       ╰──── type `Ok | Warn` has no member `value`: its members implement no common interface that declares `value`
//    │
//    │ Note: Error code: E0007
// ───╯
