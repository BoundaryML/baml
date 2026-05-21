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
// error: type mismatch: expected user.Person, got user.Named
//     ╭─[ interface_error_as_downcast_interface_to_child.baml:20:10 ]
//     │
//  20 │   return n.as<Person>.occupation
//     │          ──────┬─────  
//     │                ╰─────── type mismatch: expected user.Person, got user.Named
//     │ 
//     │ Note: Error code: E0001
// ────╯
