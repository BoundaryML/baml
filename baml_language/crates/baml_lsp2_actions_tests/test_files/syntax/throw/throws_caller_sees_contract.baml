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
// E0096
//
//   × declared throws is `Errors`, but this function may also throw `Errors.AuthError`
//    ╭─[throw_throws_caller_sees_contract.baml:9:43]
//  9 │ function MayFail(x: int) -> string throws Errors {
//    ·                                           ──────
//    ╰────
// E0097
//
//   ⚠ Unnecessary throws declaration: this function cannot throw Errors.
//    ╭─[throw_throws_caller_sees_contract.baml:9:43]
//  9 │ function MayFail(x: int) -> string throws Errors {
//    ·                                           ──────
//    ╰────
