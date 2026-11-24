// BEP-044 §"self Access": `self.upcast<InterfaceName>.method()` works from inside an
// `implements` block to reach a sibling interface's method even when names
// are flat.

interface Greeter {
  function greet(self) -> string
}

interface Farewell {
  function bye(self) -> string
}

class Polite {
  name: string

  implements Greeter {
    function greet(self) -> string {
      return "Hello, I'm " + self.name
    }
  }

  implements Farewell {
    function bye(self) -> string {
      return self.upcast<Greeter>.greet() + " — and goodbye!"
    }
  }
}
