class User {
  name string
  email string
}

class Envelope {
  payload unknown
}

function load(raw: unknown) -> string {
  // no `as`, no `any` -- prove it with `match` first
  raw.email.to_lower_case()
}

function load_nested(envelope: Envelope) -> string {
  envelope.payload.email
}

//----
//- diagnostics
// error: cannot access field `email` on `unknown`
//     ╭─[ expr_unknown_field_access.baml:12:7 ]
//     │
//  12 │   raw.email.to_lower_case()
//     │   ─┬─ ──┬──
//     │    ╰───────── this value has type `unknown`
//     │         │
//     │         ╰──── use `match` to narrow this value before accessing `email`
//     │
//     │ Note: Error code: E0007
// ────╯
// error: cannot access field `email` on `unknown`
//     ╭─[ expr_unknown_field_access.baml:16:20 ]
//     │
//  16 │   envelope.payload.email
//     │            ───┬─── ──┬──
//     │               ╰─────────── this value has type `unknown`
//     │                      │
//     │                      ╰──── use `match` to narrow this value before accessing `email`
//     │
//     │ Note: Error code: E0007
// ────╯
