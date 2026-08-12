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
// E0007
//
//   × type `unknown` has no member `email`
//     ╭─[expr_unknown_field_access.baml:12:7]
//  12 │   raw.email.to_lower_case()
//     ·   ─┬─ ──┬──
//     ·    │    ╰── use `match` to narrow this value before accessing `email`
//     ·    ╰── this value has type `unknown`
//     ╰────
//   ╰─▶   ☞ this value has type `unknown`
//           ╭─[expr_unknown_field_access.baml:12:3]
//        12 │   raw.email.to_lower_case()
//           ·   ───
//           ╰────
// E0007
//
//   × type `unknown` has no member `email`
//     ╭─[expr_unknown_field_access.baml:16:20]
//  16 │   envelope.payload.email
//     ·            ───┬─── ──┬──
//     ·               │      ╰── use `match` to narrow this value before accessing `email`
//     ·               ╰── this value has type `unknown`
//     ╰────
//   ╰─▶   ☞ this value has type `unknown`
//           ╭─[expr_unknown_field_access.baml:16:12]
//        16 │   envelope.payload.email
//           ·            ───────
//           ╰────
