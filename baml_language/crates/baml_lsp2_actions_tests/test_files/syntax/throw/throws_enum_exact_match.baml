// Throws tracks enum variants exactly; declaring the enum does not cover thrown variants.

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
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_enum_exact_match.baml:9:58]
//   9 │ ╭─▶ function ThrowsEnumExact(x: int) -> string throws Errors {
//  10 │ │     match (x) {
//  11 │ │       0 => throw Errors.AuthError,
//  12 │ │       1 => throw Errors.NotFoundError,
//  13 │ │       _ => throw Errors.InternalError
//  14 │ │     }
//  15 │ ╰─▶ }
//     ╰────
