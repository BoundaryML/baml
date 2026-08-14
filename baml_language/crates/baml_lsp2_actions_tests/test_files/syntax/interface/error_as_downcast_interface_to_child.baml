// BEP-044: `.as<I>` is for upcasts, not interface downcasts.

interface Named {
  name: string
}

interface Person requires Named {
  occupation: string
}

class Employee {
  name: string
  occupation: string

  implements Named {}
  implements Person {}
}

function bad(n: Named) -> string {
  return n.as<Person>.occupation
}

//----
//- diagnostics
// E0001
//
//   × type `Named` does not implement interface `Person`
//     ╭─[interface_error_as_downcast_interface_to_child.baml:20:10]
//  20 │   return n.as<Person>.occupation
//     ·          ────────────
//     ╰────
