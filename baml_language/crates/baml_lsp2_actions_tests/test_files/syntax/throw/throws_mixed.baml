// throws mix of primitive + enum type
// NOTE: enum variants in throws type position are not yet supported.

enum Errors {
  AuthError
  NotFoundError
}

function ThrowsMixed(x: int) -> string throws string | Errors {
  match (x) {
    0 => throw "oops",
    _ => throw Errors.AuthError
  }
}

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: Errors
//     ╭─[throw_throws_mixed.baml:9:63]
//   9 │ ╭─▶ function ThrowsMixed(x: int) -> string throws string | Errors {
//  10 │ │     match (x) {
//  11 │ │       0 => throw "oops",
//  12 │ │       _ => throw Errors.AuthError
//  13 │ │     }
//  14 │ ╰─▶ }
//     ╰────
