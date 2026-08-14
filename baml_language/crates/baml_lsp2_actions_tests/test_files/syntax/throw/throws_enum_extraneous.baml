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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_enum_extraneous.baml:9:63]
//   9 │ ╭─▶ function ThrowsEnumExtraneous(x: int) -> string throws Errors {
//  10 │ │     throw Errors.AuthError
//  11 │ ╰─▶ }
//     ╰────
