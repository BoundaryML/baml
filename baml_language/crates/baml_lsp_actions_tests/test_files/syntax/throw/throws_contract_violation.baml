// throws contract violation: inferred exceeds declared — error

function ThrowsViolation(x: int) -> string throws string {
  match (x) {
    0 => throw "error",
    1 => throw 42,
    _ => "ok"
  }
}

//----
//- diagnostics
// Error: Function throws types not covered by `throws` declaration: int
//    ╭─[ throw_throws_contract_violation.baml:3:38 ]
//    │
//  3 │ function ThrowsViolation(x: int) -> string throws string {
//    │                                      ───┬───  
//    │                                         ╰───── Function throws types not covered by `throws` declaration: int
//    │ 
//    │ Note: Error code: E0096
// ───╯
