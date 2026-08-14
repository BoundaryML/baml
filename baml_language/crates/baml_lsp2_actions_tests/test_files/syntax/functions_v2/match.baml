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
// E0063
//
//   × unreachable arm
//     ╭─[functions_v2_match.baml:32:12]
//  32 │       _ => "...",
//     ·            ─────
//     ╰────
// E0001
//
//   × mismatched types
//     ╭─[functions_v2_match.baml:31:17]
//  31 │       let rock: Rock => "...",
//     ·                 ──┬─
//     ·                   ╰── expected `Cat | Dog`, found `Rock`
//     ╰────
