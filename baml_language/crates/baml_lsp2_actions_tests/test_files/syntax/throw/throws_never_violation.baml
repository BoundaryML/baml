// throws never with escaping throws — error

function ThrowsNeverViolation(x: int) -> string throws never {
  if (x < 0) {
    throw "escaped"
  }
  "ok"
}

//----
//- diagnostics
// error: throws contract violation: `never` is missing string
//    ╭─[ throw_throws_never_violation.baml:3:55 ]
//    │
//  3 │ function ThrowsNeverViolation(x: int) -> string throws never {
//    │                                                       ───┬──  
//    │                                                          ╰──── throws contract violation: `never` is missing string
//    │ 
//    │ Note: Error code: E0096
// ───╯
