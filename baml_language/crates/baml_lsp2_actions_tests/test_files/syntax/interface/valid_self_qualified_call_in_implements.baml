// BEP-044 §"self Access": `self.as<InterfaceName>.method()` works from inside an
// `implements` block to reach a sibling interface's method even when names
// are flat.

interface Greeter {
  function greet(self) -> string throws never
}

interface Farewell {
  function bye(self) -> string throws never
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
      return self.as<Greeter>.greet() + " — and goodbye!"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
