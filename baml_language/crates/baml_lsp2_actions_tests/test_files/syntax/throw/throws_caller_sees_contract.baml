// Caller sees declared throws contract, not body-derived facts

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function MayFail(x: int) -> string throws Errors {
  throw Errors.AuthError
}

function Caller(x: int) -> string {
  MayFail(x) catch (e) {
    Errors.AuthError => "auth",
    Errors.NotFoundError => "notfound",
    Errors.InternalError => "internal"
  }
}

//----
//- diagnostics
// Error: throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    ╭─[ throw_throws_caller_sees_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError
//    │ 
//    │ Note: Error code: E0096
// ───╯
// Warning: extraneous throws declaration: user.Errors
//    ╭─[ throw_throws_caller_sees_contract.baml:9:42 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                          ───┬───  
//    │                                             ╰───── extraneous throws declaration: user.Errors
//    │ 
//    │ Note: Error code: E0097
// ───╯
// Warning: unreachable arm
//     ╭─[ throw_throws_caller_sees_contract.baml:16:29 ]
//     │
//  16 │     Errors.NotFoundError => "notfound",
//     │                             ─────┬────  
//     │                                  ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Warning: unreachable arm
//     ╭─[ throw_throws_caller_sees_contract.baml:17:29 ]
//     │
//  17 │     Errors.InternalError => "internal"
//     │                             ─────┬────  
//     │                                  ╰────── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
