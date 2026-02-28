// Caller sees variant-precise contract from callee

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function MayFail(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
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
// <no-diagnostics-expected>
