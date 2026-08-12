// Valid: a class implementing a derived interface inherits all parent contracts.

interface Named {
  name: string
}

interface Aged {
  age: int
}

interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string throws never
}

class Employee {
  name: string
  age: int
  occupation: string
  salary: float

  implements Named {}
  implements Aged {}

  implements Person {
    function introduce(self) -> string {
      return "hello"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
