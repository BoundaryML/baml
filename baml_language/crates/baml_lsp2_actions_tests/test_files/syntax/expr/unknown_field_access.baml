class User {
  name string
  email string
}

function load(raw: unknown) -> string {
  // no `as`, no `any` -- prove it with `match` first
  raw.email.to_lower_case()
}

//----
//- diagnostics
// error: cannot access field `email` on `unknown`
//    ╭─[ expr_unknown_field_access.baml:8:7 ]
//    │
//  8 │   raw.email.to_lower_case()
//    │   ─┬─ ──┬──
//    │    ╰───────── this value has type `unknown`
//    │         │
//    │         ╰──── use `match` to narrow this value before accessing `email`
//    │
//    │ Note: Error code: E0007
// ───╯
