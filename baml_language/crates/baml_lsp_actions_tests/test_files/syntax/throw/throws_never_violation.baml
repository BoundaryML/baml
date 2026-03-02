// throws never with escaping throws — error

function ThrowsNeverViolation(x: int) -> string throws never {
  if (x < 0) {
    throw "escaped"
  }
  "ok"
}

//----
//- diagnostics
// Error: Function throws types not covered by `throws` declaration: string
//    ╭─[ throw_throws_never_violation.baml:3:43 ]
//    │
//  3 │ function ThrowsNeverViolation(x: int) -> string throws never {
//    │                                           ───┬───  
//    │                                              ╰───── Function throws types not covered by `throws` declaration: string
//    │ 
//    │ Note: Error code: E0096
// ───╯
