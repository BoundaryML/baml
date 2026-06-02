// BEP-044 §"Variance is invariant in v1": Container<Dog> is not assignable
// to Container<Animal> even though Dog implements Animal.

interface Animal {
  function speak(self) -> string
}

interface Container<T> {
  function add(self, item: T) -> null
}

class Dog {
  implements Animal {
    function speak(self) -> string { return "Woof!" }
  }
}

class DogBag {
  implements Container<Dog> {
    function add(self, item: Dog) -> null { return null }
  }
}

function takes_animal_container(c: Container<Animal>) -> null {
  return null
}

function bad() -> null {
  let bag = DogBag {}
  return takes_animal_container(bag)
}

//----
//- diagnostics
// error: type mismatch: expected Container<Animal>, got DogBag
//     ╭─[ interface_error_invariant_generic_assignment.baml:30:33 ]
//     │
//  30 │   return takes_animal_container(bag)
//     │                                 ─┬─  
//     │                                  ╰─── type mismatch: expected Container<Animal>, got DogBag
//     │ 
//     │ Note: Error code: E0001
// ────╯
