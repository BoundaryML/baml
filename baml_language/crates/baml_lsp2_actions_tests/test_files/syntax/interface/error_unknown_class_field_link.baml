// E0129: the right side of `field as class_field` must name a class field.

interface Named {
  name: string
}

class Person {
  name: string

  implements Named {
    name as display_name
  }
}

//----
//- diagnostics
// E0129
//
//   × class `Person` has no field `display_name` to link for interface `Named`
//     ╭─[interface_error_unknown_class_field_link.baml:11:13]
//  11 │     name as display_name
//     ·             ────────────
//     ╰────
