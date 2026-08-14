// BEP-044: matching on an open interface without a `_` arm is non-exhaustive
// (new implementors may appear).

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
  }
}

//----
//- diagnostics
// E0062
//
//   × non-exhaustive match on type Animal; missing: Animal {}
//     ╭─[interface_error_match_without_wildcard.baml:21:10]
//  21 │ ╭─▶   return match (a) {
//  22 │ │       let d: Dog => "dog"
//  23 │ │       let c: Cat => "cat"
//  24 │ ╰─▶   }
//     ╰────
