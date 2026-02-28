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
// Warning: `throws` declaration includes types the function never throws: Errors.InternalError, Errors.NotFoundError
//    ╭─[ throw_throws_enum_extraneous.baml:9:43 ]
//    │
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    │                                           ───┬───  
//    │                                              ╰───── `throws` declaration includes types the function never throws: Errors.InternalError, Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0097
// ───╯
