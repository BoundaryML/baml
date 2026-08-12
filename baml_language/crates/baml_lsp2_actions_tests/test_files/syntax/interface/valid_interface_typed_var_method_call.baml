// BEP-044 §"Interface as a Type": holding a value at an interface type
// selects that interface's vtable; methods are callable directly.

interface Animal {
  name: string
  function speak(self) -> string throws never
}

class Dog {
  name: string

  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function pet(a: Animal) -> string {
  return a.name + " says " + a.speak()
}

//----
//- diagnostics
// <no-diagnostics-expected>
