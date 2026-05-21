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
// error: type mismatch: expected user.Pet, got user.Rock
//     ╭─[ functions_v2_match.baml:30:32 ]
//     │
//  30 │ ╭─▶       let dog: Dog => "Bowwow",
//  31 │ ├─▶       let rock: Rock => "...",
//     │ │                                    
//     │ ╰──────────────────────────────────── type mismatch: expected user.Pet, got user.Rock
//     │     
//     │     Note: Error code: E0001
// ────╯
// error: unreachable arm
//     ╭─[ functions_v2_match.baml:31:25 ]
//     │
//  31 │       let rock: Rock => "...",
//     │                         ──┬──  
//     │                           ╰──── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
// error: unreachable arm
//     ╭─[ functions_v2_match.baml:32:12 ]
//     │
//  32 │       _ => "...",
//     │            ──┬──  
//     │              ╰──── unreachable arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
