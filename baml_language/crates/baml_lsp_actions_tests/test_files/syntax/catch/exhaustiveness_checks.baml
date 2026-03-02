class Dog {
  type "dog"
}

class Cat {
  type "cat"
}

type Pet = Cat | Dog

function MayFail(x: int) -> string {
  "ok"
}

function UnreachableMatchArmInsideCatch(x: int, pet: Pet) -> string {
  MayFail(x) catch (e) {
    _ => match (pet) {
      c: Cat => "cat",
      d: Dog => "dog",
      _ => "unreachable"
    }
  }
}

function NonExhaustiveMatchInsideCatch(x: int, pet: Pet) -> string {
  MayFail(x) catch (e) {
    _ => match (pet) {
      c: Cat => "cat"
    }
  }
}

//----
//- diagnostics
// Warning: Unreachable catch arm
//     ╭─[ catch_exhaustiveness_checks.baml:27:5 ]
//     │
//  27 │     _ => match (pet) {
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Error: Non-exhaustive match on `Pet`: missing cases Dog
//     ╭─[ catch_exhaustiveness_checks.baml:27:9 ]
//     │
//  27 │ ╭─▶     _ => match (pet) {
//     ┆ ┆   
//  29 │ ├─▶     }
//     │ │           
//     │ ╰─────────── Non-exhaustive match on `Pet`: missing cases Dog
//     │     
//     │     Note: Error code: E0062
// ────╯
// Warning: Unreachable catch arm
//     ╭─[ catch_exhaustiveness_checks.baml:17:5 ]
//     │
//  17 │     _ => match (pet) {
//     │     ┬  
//     │     ╰── Unreachable catch arm
//     │ 
//     │ Note: Error code: E0095
// ────╯
// Error: Unreachable match arm
//     ╭─[ catch_exhaustiveness_checks.baml:20:7 ]
//     │
//  20 │       _ => "unreachable"
//     │       ┬  
//     │       ╰── Unreachable match arm
//     │ 
//     │ Note: Error code: E0063
// ────╯
