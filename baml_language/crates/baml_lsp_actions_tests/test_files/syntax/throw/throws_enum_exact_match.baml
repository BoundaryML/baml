// throws whole enum, body throws all variants — no diagnostic

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsEnumExact(x: int) -> string throws Errors {
  match (x) {
    0 => throw Errors.AuthError,
    1 => throw Errors.NotFoundError,
    _ => throw Errors.InternalError
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
