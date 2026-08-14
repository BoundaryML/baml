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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_caller_variant_contract.baml:10:50]
//  10 │ ╭─▶ function MayFail(x: int) -> string throws Errors {
//  11 │ │     match (x) {
//  12 │ │       0 => throw Errors.AuthError,
//  13 │ │       _ => throw Errors.NotFoundError
//  14 │ │     }
//  15 │ ╰─▶ }
//     ╰────
