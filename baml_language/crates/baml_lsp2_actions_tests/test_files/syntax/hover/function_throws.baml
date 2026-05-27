enum Errors {
  AuthError
  NotFoundError
}

function MayFail<[CURSOR](x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// error: declared throws is `Errors`, but this function may also throw `user.Errors.AuthError`
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── declared throws is `Errors`, but this function may also throw `user.Errors.AuthError`
//    │ 
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: user.Errors
//    ╭─[ hover_function_throws.baml:6:42 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0097
// ───╯
//
//- on_hover expressions
// hover at hover_function_throws.baml:6:17
// No hover content
