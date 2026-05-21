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
// error: throws contract violation: `string` is missing int
//    ╭─[ throw_throws_contract_violation.baml:3:50 ]
//    │
//  3 │ function ThrowsViolation(x: int) -> string throws string {
//    │                                                  ───┬───  
//    │                                                     ╰───── throws contract violation: `string` is missing int
//    │ 
//    │ Note: Error code: E0096
// ───╯
