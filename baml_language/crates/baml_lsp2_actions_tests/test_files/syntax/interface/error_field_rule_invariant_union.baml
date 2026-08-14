// BEP-044: interface fields are invariant because interface views can write
// through to the class storage.

interface Measured {
  value: int | string
}

class Count {
  value: int
  implements Measured {}
}

//----
//- diagnostics
// E0116
//
//   × field `value` has type int, but interface `Measured` declares it as int | string
//     ╭─[interface_error_field_rule_invariant_union.baml:10:14]
//  10 │   implements Measured {}
//     ·              ────────
//     ╰────
