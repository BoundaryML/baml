// BEP-044: `.as<I>` may project from a child interface value to a required
// parent interface.

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

function read_name(p: Person) -> string {
  return p.as<Named>.name
}

//----
//- diagnostics
// <no-diagnostics-expected>
