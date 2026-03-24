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
// Error: unreachable arm
//      ╭─[ llm_types.baml:159:17 ]
//      │
//  159 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:189:17 ]
//      │
//  189 │ ╭─▶             _ => {
//      ┆ ┆   
//  194 │ ├─▶             }
//      │ │                   
//      │ ╰─────────────────── unreachable arm
//      │     
//      │     Note: Error code: E0001
// ─────╯
// Error: unreachable arm
//      ╭─[ llm_types.baml:208:17 ]
//      │
//  208 │             _ => { continue; }
//      │                 ───────┬──────  
//      │                        ╰──────── unreachable arm
//      │ 
//      │ Note: Error code: E0001
// ─────╯
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: throws contract violation: `user.AuthError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    ╭─[ throw_throws_enum_variant_violation.baml:9:59 ]
//    │
//  9 │ function ThrowsVariantViolation(x: int) -> string throws Errors.AuthError {
//    │                                                           ────────┬────────  
//    │                                                                   ╰────────── throws contract violation: `user.AuthError` is missing user.Errors.AuthError, user.Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0001
// ───╯
