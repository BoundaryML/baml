// throws mix of primitive + enum type
// NOTE: enum variants in throws type position are not yet supported.

enum Errors {
  AuthError
  NotFoundError
}

function ThrowsMixed(x: int) -> string throws string | Errors {
  match (x) {
    0 => throw "oops",
    _ => throw Errors.AuthError
  }
}

//----
//- diagnostics
// error: throws contract violation: `string | user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_mixed.baml:9:46 ]
//    │
//  9 │ function ThrowsMixed(x: int) -> string throws string | Errors {
//    │                                              ────────┬───────  
//    │                                                      ╰───────── throws contract violation: `string | user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_mixed.baml:9:46 ]
//    │
//  9 │ function ThrowsMixed(x: int) -> string throws string | Errors {
//    │                                              ────────┬───────  
//    │                                                      ╰───────── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0097
// ───╯
