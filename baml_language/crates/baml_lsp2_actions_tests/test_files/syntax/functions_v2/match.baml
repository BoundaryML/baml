class Dog {
  type "dog"
  name string
  kennel string
}

class Cat {
  type "cat"
  name string
  last_catnip_epoch_seconds int
}

type Pet = Cat | Dog

class Rock {
  weight int
}

class Situation {
  pets Pet[]
  rocks Rock[]
}

// Should throw one error:
// rock has a type mismatch, so usefulness analysis is skipped.
function Remand(s: Situation) -> string {
  if (s.pets.length() > 0) {
    match (s.pets[0]) {
      let cat: Cat => "Nice cat you got there.",
      let dog: Dog => "Bowwow",
      let rock: Rock => "...",
      _ => "...",
    }
  }
  "You should really get a pet."
}

//----
//- diagnostics
// error: type mismatch: expected Pet, got Rock
//     ╭─[ functions_v2_match.baml:31:7 ]
//     │
//  31 │       let rock: Rock => "...",
//     │       ───────┬──────
//     │              ╰──────── type mismatch: expected Pet, got Rock
//     │
//     │ Note: Error code: E0001
// ────╯
