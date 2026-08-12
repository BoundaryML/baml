// E0130: the same interface field cannot be linked more than once.

interface Named {
  name: string
}

class Person {
  primary_name: string
  secondary_name: string

  implements Named {
    name as primary_name
    name as secondary_name
  }
}

//----
//- diagnostics
// E0130
//
//   × field `name` of interface `Named` is linked more than once
//     ╭─[interface_error_duplicate_interface_field_link.baml:12:5]
//  12 │     name as primary_name
//     ·     ────
//     ╰────
// E0130
//
//   × field `name` of interface `Named` is linked more than once
//     ╭─[interface_error_duplicate_interface_field_link.baml:13:5]
//  13 │     name as secondary_name
//     ·     ────
//     ╰────
