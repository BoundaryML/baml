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
// error: declared throws is `Errors`, but this function may also throw `Errors.AuthError`
//    ╭─[ throw_throws_caller_sees_contract.baml:9:43 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                           ───┬──
//    │                                              ╰──── declared throws is `Errors`, but this function may also throw `Errors.AuthError`
//    │
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: Errors
//    ╭─[ throw_throws_caller_sees_contract.baml:9:43 ]
//    │
//  9 │ function MayFail(x: int) -> string throws Errors {
//    │                                           ───┬──
//    │                                              ╰──── extraneous throws declaration: Errors
//    │
//    │ Note: Error code: E0097
// ───╯
