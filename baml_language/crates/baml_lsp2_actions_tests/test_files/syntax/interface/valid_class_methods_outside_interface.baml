// BEP-044 §"Class Methods Outside Interfaces": a class may declare methods
// outside any `implements` block. Those methods are callable as instance
// methods alongside interface methods.

interface Configurable {
  function configure(self) -> null throws never
}

class Server {
  host: string

  function address(self) -> string {
    return self.host
  }

  implements Configurable {
    function configure(self) -> null {
      let _addr = self.address()
      return null
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
