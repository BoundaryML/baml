// throws enum type, but body throws variants — contract mismatch
// NOTE: enum variants in throws type position are not yet supported.

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsVariantViolation(x: int) -> string throws Errors {
  match (x) {
    0 => throw Errors.AuthError,
    _ => throw Errors.NotFoundError
  }
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_enum_variant_violation.baml:10:65]
//  10 │ ╭─▶ function ThrowsVariantViolation(x: int) -> string throws Errors {
//  11 │ │     match (x) {
//  12 │ │       0 => throw Errors.AuthError,
//  13 │ │       _ => throw Errors.NotFoundError
//  14 │ │     }
//  15 │ ╰─▶ }
//     ╰────
