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
// error: throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.NotFoundError
//     ╭─[ throw_throws_enum_variant_precise.baml:12:53 ]
//     │
//  12 │ function ThrowsVariantExact(x: int) -> string throws Errors {
//     │                                                     ───┬───  
//     │                                                        ╰───── throws contract violation: `user.Errors` is missing user.Errors.AuthError, user.Errors.NotFoundError
//     │ 
//     │ Note: Error code: E0096
// ────╯
// warning: extraneous throws declaration: user.Errors
//     ╭─[ throw_throws_enum_variant_precise.baml:12:53 ]
//     │
//  12 │ function ThrowsVariantExact(x: int) -> string throws Errors {
//     │                                                     ───┬───  
//     │                                                        ╰───── extraneous throws declaration: user.Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
