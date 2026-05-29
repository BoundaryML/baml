// BEP-044 §"Requires with conflicting field types": the error is on the child
// interface itself, not on implementors.

interface X {
  id: string
}

interface Y {
  id: int
}

interface Z requires X, Y {}

//----
//- diagnostics
// error: interface `user.Z` inherits conflicting types for field `id`: `string` from `user.X`, `int` from `user.Y`
//     ╭─[ interface_error_interface_extends_field_conflict.baml:12:11 ]
//     │
//  12 │ interface Z requires X, Y {}
//     │           ┬  
//     │           ╰── conflicting field types inherited via `requires`
//     │ 
//     │ Note: Error code: E0122
// ────╯
