// throws mix of primitive + enum variant — no diagnostic when body matches

enum Errors {
  AuthError
  NotFoundError
}

function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
  match (x) {
    0 => throw "oops",
    _ => throw Errors.AuthError
  }
}

//----
//- diagnostics
// Error: throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── throws contract violation: `string | user.AuthError` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
// Error: extraneous throws declaration: user.AuthError
//    ╭─[ throw_throws_mixed.baml:8:48 ]
//    │
//  8 │ function ThrowsMixed(x: int) -> string throws string | Errors.AuthError {
//    │                                                ─────────────┬────────────  
//    │                                                             ╰────────────── extraneous throws declaration: user.AuthError
//    │ 
//    │ Note: Error code: E0001
// ───╯
