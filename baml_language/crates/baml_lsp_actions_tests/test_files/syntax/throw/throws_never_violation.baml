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
// Warning: `throws` declaration includes types the function never throws: never
//    ╭─[ throw_throws_never_violation.baml:3:43 ]
//    │
//  3 │ function ThrowsNeverViolation(x: int) -> string throws never {
//    │                                           ───┬───  
//    │                                              ╰───── `throws` declaration includes types the function never throws: never
//    │ 
//    │ Note: Error code: E0097
// ───╯
