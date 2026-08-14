// BEP-044 §"Interaction with match": open interfaces require a `_` arm. With
// one present the match is exhaustive.

interface Animal {
  function speak(self) -> string throws never
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

class Cat {
  implements Animal {
    function speak(self) -> string { return "Meow." }
  }
}

function describe(a: Animal) -> string {
  return match (a) {
    let d: Dog => "dog"
    let c: Cat => "cat"
    _ => "other"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
