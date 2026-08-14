// Valid: class implements an interface with a required method.

interface Animal {
  name: string
  age: int

  function speak(self) -> string throws never
}

class Dog {
  name: string
  age: int
  breed: string

  implements Animal {
    function speak(self) -> string {
      return "Woof!"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
