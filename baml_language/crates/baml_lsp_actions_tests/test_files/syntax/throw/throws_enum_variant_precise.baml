// throws with specific enum variants — exact match, no diagnostic

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsVariantExact(x: int) -> string throws Errors.AuthError | Errors.NotFoundError {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
