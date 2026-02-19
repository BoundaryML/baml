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

// Should throw two errors:
//  rock not reachable, _ not reachable.
function Remand(s: Situation) -> string {
  if (s.pets.length() > 0) {
    match (s.pets[0]) {
      cat: Cat => "Nice cat you got there.",
      dog: Dog => "Bowwow",
      rock: Rock => "...",
      _ => "...",
    }
  }
  "You should really get a pet."
}

//----
//- diagnostics
// Error: Unreachable match arm
//     ╭─[ functions_v2_match.baml:31:7 ]
//     │
//  31 │       rock: Rock => "...",
//     │       ─────┬────  
//     │            ╰────── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// Error: Unreachable match arm
//     ╭─[ functions_v2_match.baml:32:7 ]
//     │
//  32 │       _ => "...",
//     │       ┬  
//     │       ╰── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
