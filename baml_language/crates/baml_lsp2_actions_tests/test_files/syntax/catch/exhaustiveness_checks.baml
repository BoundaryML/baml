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
// Error: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:17:9 ]
//     │
//  17 │ ╭─▶     _ => match (pet) {
//     ┆ ┆   
//  21 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
// Error: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:27:9 ]
//     │
//  27 │ ╭─▶     _ => match (pet) {
//     ┆ ┆   
//  29 │ ├─▶     }
//     │ │           
//     │ ╰─────────── unreachable arm
//     │     
//     │     Note: Error code: E0001
// ────╯
