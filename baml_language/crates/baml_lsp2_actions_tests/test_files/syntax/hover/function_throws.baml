enum Errors {
  AuthError
  NotFoundError
}

function MayFail<[CURSOR](x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//    ╭─[hover_function_throws.baml:6:50]
//  6 │ ╭─▶ function MayFail(x: int) -> string throws Errors {
//  7 │ │     throw Errors.AuthError
//  8 │ ╰─▶ }
//    ╰────
//
//- on_hover expressions
// hover at hover_function_throws.baml:6:17
// No hover content
