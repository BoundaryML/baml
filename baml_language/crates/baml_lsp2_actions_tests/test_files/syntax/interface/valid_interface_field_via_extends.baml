// BEP-044: fields contributed by a grandparent interface should flow into
// the implementor through the full requires chain.

interface Named { name: string }
interface Aged { age: int }
interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string throws never {
    return self.name + ", " + self.occupation
  }
}

class Employee {
  name: string
  age: int
  occupation: string
  salary: float

  implements Named {}
  implements Aged {}
  implements Person {}
}

function get_name(e: Employee) -> string {
  return e.name
}

function get_age(e: Employee) -> int {
  return e.age
}

//----
//- diagnostics
// <no-diagnostics-expected>
