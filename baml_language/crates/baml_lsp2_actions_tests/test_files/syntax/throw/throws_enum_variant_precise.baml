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
// E0096
//
//   × declared throws is `Errors`, but this function may also throw `Errors.AuthError | Errors.NotFoundError`
//     ╭─[throw_throws_enum_variant_precise.baml:12:54]
//  12 │ function ThrowsVariantExact(x: int) -> string throws Errors {
//     ·                                                      ──────
//     ╰────
// E0097
//
//   ⚠ Unnecessary throws declaration: this function cannot throw Errors.
//     ╭─[throw_throws_enum_variant_precise.baml:12:54]
//  12 │ function ThrowsVariantExact(x: int) -> string throws Errors {
//     ·                                                      ──────
//     ╰────
