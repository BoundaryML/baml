enum Errors {
  AuthError
  NotFoundError
}

function MayFail<[CURSOR](x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// Warning: `throws` declaration includes types the function never throws: Errors.NotFoundError
//    ╭─[ hover_function_throws.baml:6:28 ]
//    │
//  6 │ function MayFail(x: int) -> string throws Errors {
//    │                            ───┬───  
//    │                               ╰───── `throws` declaration includes types the function never throws: Errors.NotFoundError
//    │ 
//    │ Note: Error code: E0097
// ───╯
//
//- on_hover expressions
// hover at hover_function_throws.baml:6:17
// function MayFail(x: int) -> string throws Errors
