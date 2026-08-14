// throws contract violation: inferred exceeds declared — error

function ThrowsViolation(x: int) -> string throws string {
  match (x) {
    0 => throw "error",
    1 => throw 42,
    _ => "ok"
  }
}

//----
//- diagnostics
// E0096
//
//   × declared throws is `string`, but this function may also throw `int`
//    ╭─[throw_throws_contract_violation.baml:6:16]
//  6 │     1 => throw 42,
//    ·                ──
//    ╰────
