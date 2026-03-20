// throws with specific enum variants — exact match, no diagnostic

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

//----
//- diagnostics
// Error: throws contract violation: `user.AuthError | user.NotFoundError` is missing Errors.AuthError, Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_precise.baml:9:55 ]
//    │
//  9 │ function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                                       ────────────────────┬───────────────────  
//    │                                                                           ╰───────────────────── throws contract violation: `user.AuthError | user.NotFoundError` is missing Errors.AuthError, Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: AuthError, NotFoundError
//    ╭─[ throw_throws_enum_variant_precise.baml:9:55 ]
//    │
//  9 │ function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
//    │                                                       ────────────────────┬───────────────────  
//    │                                                                           ╰───────────────────── extraneous throws declaration: AuthError, NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
