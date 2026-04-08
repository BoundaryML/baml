// throws with enum type — exact match
// NOTE: enum variants in throws type position are not yet supported;
// using the enum type directly still produces contract mismatch because
// the throws checker compares Ty::Enum vs Ty::EnumVariant.

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsVariantExact(x: int) -> string throws Errors {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
