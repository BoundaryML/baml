// E0128: the left side of `field as class_field` must name an interface field.

interface Named {
  name: string
}

class Person {
  name: string

  implements Named {
    title as name
  }
}

//----
//- diagnostics
// E0128
//
//   × interface `Named` has no field `title` to link
//     ╭─[interface_error_unknown_interface_field_link.baml:11:5]
//  11 │     title as name
//     ·     ─────
//     ╰────
