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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_enum_variant_precise.baml:12:61]
//  12 │ ╭─▶ function ThrowsVariantExact(x: int) -> string throws Errors {
//  13 │ │     match (x) {
//  14 │ │       0 => throw Errors.AuthError,
//  15 │ │       _ => throw Errors.NotFoundError
//  16 │ │     }
//  17 │ ╰─▶ }
//     ╰────
