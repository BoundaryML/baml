// BEP-044: `Self` is only valid inside a class or interface declaration.

function bad(x: Self) -> Self {
  return x
}

//----
//- diagnostics
// E0002
//
//   × unresolved type: Self
//    ╭─[interface_error_self_outside_type.baml:3:17]
//  3 │ function bad(x: Self) -> Self {
//    ·                 ────
//    ╰────
// E0002
//
//   × unresolved type: Self
//    ╭─[interface_error_self_outside_type.baml:3:26]
//  3 │ function bad(x: Self) -> Self {
//    ·                          ────
//    ╰────
