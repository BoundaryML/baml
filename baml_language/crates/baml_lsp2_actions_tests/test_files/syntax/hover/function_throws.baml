enum Errors {
  AuthError
  NotFoundError
}

function MayFail<[CURSOR](x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// E0096
//
//   × declared throws is `Errors`, but this function may also throw `Errors.AuthError`
//    ╭─[hover_function_throws.baml:6:43]
//  6 │ function MayFail(x: int) -> string throws Errors {
//    ·                                           ──────
//    ╰────
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//    ╭─[hover_function_throws.baml:6:43]
//  6 │ function MayFail(x: int) -> string throws Errors {
//    ·                                           ──────
//    ╰────
//
//- on_hover expressions
// hover at hover_function_throws.baml:6:17
// No hover content
