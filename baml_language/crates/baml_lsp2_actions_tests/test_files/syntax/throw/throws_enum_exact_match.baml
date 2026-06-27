// throws whole enum, body throws all variants — no diagnostic

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
// error: declared throws is `Errors`, but this function may also throw `Errors.AuthError | Errors.InternalError | Errors.NotFoundError`
//    ╭─[ throw_throws_enum_exact_match.baml:9:51 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                   ───┬──
//    │                                                      ╰──── declared throws is `Errors`, but this function may also throw `Errors.AuthError | Errors.InternalError | Errors.NotFoundError`
//    │
//    │ Note: Error code: E0096
// ───╯
// warning: extraneous throws declaration: Errors
//    ╭─[ throw_throws_enum_exact_match.baml:9:51 ]
//    │
//  9 │ function ThrowsEnumExact(x: int) -> string throws Errors {
//    │                                                   ───┬──
//    │                                                      ╰──── extraneous throws declaration: Errors
//    │
//    │ Note: Error code: E0097
// ───╯
