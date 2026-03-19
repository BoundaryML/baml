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
// Error: throws contract violation: `user.AuthError` is missing Errors.AuthError, Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── throws contract violation: `user.AuthError` is missing Errors.AuthError, Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: AuthError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── extraneous throws declaration: AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
