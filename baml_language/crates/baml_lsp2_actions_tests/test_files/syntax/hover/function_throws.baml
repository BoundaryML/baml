enum Errors {
  AuthError
  NotFoundError
}

function MayFail<[CURSOR](x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// Error: throws contract violation: `user.Errors` is missing Errors.AuthError
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── throws contract violation: `user.Errors` is missing Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: Errors
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: Errors
//    │ 
//    │ Note: Error code: E0001
// ───╯
//
//- on_hover expressions
// hover at hover_function_throws.baml:6:17
// No hover content
