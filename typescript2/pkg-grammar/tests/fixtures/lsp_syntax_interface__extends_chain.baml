// BEP-044: a class implementing the leaf interface in a three-level requires
// chain should satisfy every contract in the chain. All fields and methods
// flow down transitively.

interface HasName {
  name: string
}

interface Greeter requires HasName {
  function greet(self) -> string
}

interface Polite requires Greeter {
  function farewell(self) -> string
}

class Robot {
  name: string

  implements HasName {}

  implements Greeter {
    function greet(self) -> string { return "hello" }
  }

  implements Polite {
    function farewell(self) -> string { return "bye" }
  }
}
