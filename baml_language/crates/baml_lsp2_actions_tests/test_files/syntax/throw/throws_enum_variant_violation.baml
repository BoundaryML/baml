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
// error: declared throws is `Errors`, but this function may also throw `Errors.AuthError | Errors.NotFoundError`
//     ╭─[ throw_throws_enum_variant_violation.baml:10:57 ]
//     │
//  10 │ function ThrowsVariantViolation(x: int) -> string throws Errors {
//     │                                                         ───┬───  
//     │                                                            ╰───── declared throws is `Errors`, but this function may also throw `Errors.AuthError | Errors.NotFoundError`
//     │ 
//     │ Note: Error code: E0096
// ────╯
// warning: extraneous throws declaration: Errors
//     ╭─[ throw_throws_enum_variant_violation.baml:10:57 ]
//     │
//  10 │ function ThrowsVariantViolation(x: int) -> string throws Errors {
//     │                                                         ───┬───  
//     │                                                            ╰───── extraneous throws declaration: Errors
//     │ 
//     │ Note: Error code: E0097
// ────╯
