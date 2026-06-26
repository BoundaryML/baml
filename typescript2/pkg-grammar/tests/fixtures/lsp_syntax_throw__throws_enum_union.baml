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
