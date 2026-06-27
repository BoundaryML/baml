// BEP-044: matching on an open interface without a `_` arm is non-exhaustive
// (new implementors may appear).

interface Animal {
  function speak(self) -> string
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
// error: non-exhaustive match on type Animal; missing: _
//     ╭─[ interface_error_match_without_wildcard.baml:21:10 ]
//     │
//  21 │ ╭─▶   return match (a) {
//     ┆ ┆
//  24 │ ├─▶   }
//     │ │
//     │ ╰───────── non-exhaustive match on type Animal; missing: _
//     │
//     │     Note: Error code: E0062
// ────╯
