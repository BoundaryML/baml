// BEP-044: when two interface views expose the same field name through
// different links, a concrete receiver must project with `.as<Interface>`.

interface Named {
  name: string
}

interface Labeled {
  name: string
}

class Item {
  named_name: string
  label_name: string

  implements Named {
    name as named_name
  }
  implements Labeled {
    name as label_name
  }
}

function bad(i: Item) -> string {
  return i.name
}

//----
//- diagnostics
// E0131
//
//   × field `name` on class `Item` is ambiguous because it is declared by multiple interfaces: `Named`, `Labeled`; use obj.as<Named>.name or obj.as<Labeled>.name
//     ╭─[interface_error_ambiguous_interface_field_access.baml:25:10]
//  25 │   return i.name
//     ·          ──────
//     ╰────
