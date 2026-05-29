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
// error: class `Count` declares field `value: int` but interface `user.Measured` requires `value` to have type `int | string`
//    ╭─[ interface_error_field_rule_invariant_union.baml:9:9 ]
//    │
//  9 │   value: int
//    │         ──┬─  
//    │           ╰─── expected `int | string`, found `int`
//    │ 
//    │ Note: Error code: E0116
// ───╯
