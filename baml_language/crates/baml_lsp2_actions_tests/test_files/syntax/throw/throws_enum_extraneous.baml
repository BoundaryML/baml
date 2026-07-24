// throws whole enum, body only throws one variant — E0097 for unused variants

enum Errors {
  AuthError
  NotFoundError
  InternalError
}

function ThrowsEnumExtraneous(x: int) -> string throws Errors {
  throw Errors.AuthError
}

//----
//- diagnostics
// E0096
//
//   × declared throws is `Errors`, but this function may also throw `Errors.AuthError`
//    ╭─[throw_throws_enum_extraneous.baml:9:56]
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    ·                                                        ──────
//    ╰────
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//    ╭─[throw_throws_enum_extraneous.baml:9:56]
//  9 │ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//    ·                                                        ──────
//    ╰────
