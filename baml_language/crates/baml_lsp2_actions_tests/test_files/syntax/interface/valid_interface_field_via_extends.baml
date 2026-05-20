// BEP-044: fields contributed by a grandparent interface should flow into
// the implementor through the full requires chain.

interface Named { name: string }
interface Aged { age: int }
interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string {
    return self.name + ", " + self.occupation
  }
}

class Employee {
  salary: float

  implements Named {
    name: string
  }

  implements Aged {
    age: int
  }

  implements Person {
    occupation: string
  }
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
