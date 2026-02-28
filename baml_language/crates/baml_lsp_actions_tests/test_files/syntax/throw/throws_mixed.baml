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
// <no-diagnostics-expected>
