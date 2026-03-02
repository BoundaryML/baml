enum Errors {
  AuthError
  NotFoundError
}

function MayFail(x: int) -> string {
  match (x) {
    0 => throw Errors.AuthError,
    1 => throw Errors.NotFoundError,
    _ => "ok"
  }
}

function Handler(x: int) -> string {
  MayFail(x) catch (<[CURSOR]e) {
    Errors.AuthError => { throw e }
    Errors.NotFoundError => "notfound"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- on_hover expressions
// hover at hover_catch_binding.baml:15:21
// let e: Errors.AuthError | Errors.NotFoundError
