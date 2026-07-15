// throws never with escaping throws — error

function ThrowsNeverViolation(x: int) -> string throws never {
  if (x < 0) {
    throw "escaped"
  }
  "ok"
}

//----
//- diagnostics
// error: declared throws is `never`, but this function may also throw `string`
//    ╭─[ throw_throws_never_violation.baml:3:56 ]
//    │
//  3 │ function ThrowsNeverViolation(x: int) -> string throws never {
//    │                                                        ──┬──
//    │                                                          ╰──── declared throws is `never`, but this function may also throw `string`
//    │
//    │ Note: Error code: E0096
// ───╯
