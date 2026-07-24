// BEP-044: inside `a: Dog => ...`, the binding is typed as `Dog` and can
// access concrete-class fields like `breed` that aren't on the interface.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  breed: string
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

function describe(a: Animal) -> string {
  return match (a) {
    let d: Dog => d.breed
    _ => "other"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
