// throws specific variant, but body throws an extra variant — E0096

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

//----
//- diagnostics
// Error: Function throws types not covered by `throws` declaration: Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:45 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                             ───┬───  
//    │                                                ╰───── Function throws types not covered by `throws` declaration: Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0096
// ───╯
