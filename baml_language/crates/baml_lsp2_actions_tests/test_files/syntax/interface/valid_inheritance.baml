// Valid: a class implementing a derived interface inherits all parent contracts.

interface Named {
  name: string
}

interface Aged {
  age: int
}

interface Person requires Named, Aged {
  occupation: string

  function introduce(self) -> string
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

    function introduce(self) -> string {
      return "hello"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
