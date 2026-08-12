// BEP-044: `implement` (singular) is accepted as an alias for `implements`.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implement Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
