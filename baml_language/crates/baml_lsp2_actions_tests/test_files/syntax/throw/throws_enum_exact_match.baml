// throws whole enum, body throws all variants — no diagnostic

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsEnumExact(x: int) -> string throws Errors {
  match (x) {
    0 => throw Errors.AuthError,
    1 => throw Errors.NotFoundError,
    _ => throw Errors.InternalError
  }
}

//----
//- diagnostics
// error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.InternalError, user.Errors.NotFoundError
//    ╭─[ throw_throws_enum_exact_match.baml:9:50 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                  ───┬───  
//    │                                                     ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.InternalError, user.Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_exact_match.baml:9:50 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                  ───┬───  
//    │                                                     ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0097
// ───╯
