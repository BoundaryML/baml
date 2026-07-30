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
// E0096
//
//   × declared throws is `string | Errors`, but this function may also throw `Errors.AuthError`
//    ╭─[throw_throws_mixed.baml:9:47]
//  9 │ function ThrowsMixed(x: int) -> string throws string | Errors {
//    ·                                               ───────────────
//    ╰────
// E0097
//
//   ⚠ Unnecessary throws declaration: this function cannot throw Errors.
//    ╭─[throw_throws_mixed.baml:9:47]
//  9 │ function ThrowsMixed(x: int) -> string throws string | Errors {
//    ·                                               ───────────────
//    ╰────
