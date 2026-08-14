// throws never with escaping throws — error

function ThrowsNeverViolation(x: int) -> string throws never {
  if (x < 0) {
    throw "escaped"
  }
  "ok"
}

//----
//- diagnostics
// E0096
//
//   × declared throws is `never`, but this function may also throw `string`
//    ╭─[throw_throws_never_violation.baml:5:11]
//  5 │     throw "escaped"
//    ·           ─────────
//    ╰────
