// BEP-044: `Self` is only valid inside a class or interface declaration.

function bad(x: Self) -> Self {
  return x
}

//----
//- diagnostics
// error: unresolved type: Self
//    ╭─[ interface_error_self_outside_type.baml:3:25 ]
//    │
//  3 │ function bad(x: Self) -> Self {
//    │                         ──┬──  
//    │                           ╰──── unresolved type: Self
//    │ 
//    │ Note: Error code: E0002
// ───╯
// error: unresolved type: Self
//    ╭─[ interface_error_self_outside_type.baml:3:16 ]
//    │
//  3 │ function bad(x: Self) -> Self {
//    │                ──┬──  
//    │                  ╰──── unresolved type: Self
//    │ 
//    │ Note: Error code: E0002
// ───╯
