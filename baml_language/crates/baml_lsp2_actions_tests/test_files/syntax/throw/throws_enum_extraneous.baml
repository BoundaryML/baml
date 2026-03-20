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
// Error: throws contract violation: `user.Errors` is missing Errors.AuthError
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── throws contract violation: `user.Errors` is missing Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: Errors
//    ╭─[ throw_throws_enum_extraneous.baml:9:57 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                                         ───┬───  
//    │                                                            ╰───── extraneous throws declaration: Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
