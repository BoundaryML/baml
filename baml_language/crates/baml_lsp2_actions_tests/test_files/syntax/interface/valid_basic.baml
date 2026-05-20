// Valid: class implements an interface with a required method.

interface Animal {
  name: string
  age: int

  function speak(self) -> string
}

class Dog {
  breed: string

  implements Animal {
    name: string
    age: int

    function speak(self) -> string {
      return "Woof!"
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
