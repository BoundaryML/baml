// throws whole enum, body only throws one variant — E0097 for unused variants

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsEnumExtraneous(x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_enum_extraneous.baml:9:55 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                       ───┬───  
//    │                                                          ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_enum_extraneous.baml:9:55 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                       ───┬───  
//    │                                                          ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0097
// ───╯
