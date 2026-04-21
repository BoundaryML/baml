// Caller sees enum-type contract from callee
// NOTE: enum variants in throws type position are not yet supported.

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function MayFail(x: int) -> string throws Errors {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

function CallerGood(x: int) -> string {
  MayFail(x) catch (e) {
    Errors.AuthError => "auth",
    Errors.NotFoundError => "notfound"
  }
}

//----
//- diagnostics
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.NotFoundError
//     ╭─[ throw_throws_caller_variant_contract.baml:10:42 ]
//     │
//  10 │ function MayFail(x: int) -> string throws Errors {
//     │                                          ───┬───  
//     │                                             ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.NotFoundError
//     │ 
//     │ Note: Error code: E0096
// ────╯
// Warning: extraneous throws declaration: user.Errors
//     ╭─[ throw_throws_caller_variant_contract.baml:10:42 ]
//     │
//  10 │ function MayFail(x: int) -> string throws Errors {
//     │                                          ───┬───  
//     │                                             ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
// Warning: unreachable arm
//     ╭─[ throw_throws_caller_variant_contract.baml:20:29 ]
//     │
//  20 │     Errors.NotFoundError => "notfound"
//     │                             ─────┬────  
//     │                                  ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
