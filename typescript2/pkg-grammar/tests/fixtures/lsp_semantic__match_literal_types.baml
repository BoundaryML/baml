class Dog {
  type "dog"
  name string
}

class Cat {
  type "cat"
  name string
}

class Lizard {
  type "lizard"
  name string
}

type Pet = Cat | Dog | Lizard;

function Describe(pet: Pet) -> string {
  match (pet) {
    let cat: Cat => cat.name,
    let dog: Dog => dog.name,
    _ => "other pet",
  }
}
