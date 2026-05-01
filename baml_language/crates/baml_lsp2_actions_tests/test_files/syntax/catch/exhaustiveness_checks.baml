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
      let c: Cat => "cat",
      let d: Dog => "dog",
      _ => "unreachable"
    }
  }
}

function NonExhaustiveMatchInsideCatch(x: int, pet: Pet) -> string {
  MayFail(x) catch (e) {
    _ => match (pet) {
      let c: Cat => "cat"
    }
  }
}

//----
//- diagnostics
// Warning: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:17:9 ]
//     │
//  17 │ ╭─▶     _ => match (pet) {
//     ┆ ┆   
//  21 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0063
// ────╯
// Warning: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:27:9 ]
//     │
//  27 │ ╭─▶     _ => match (pet) {
//     ┆ ┆   
//  29 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0063
// ────╯
