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
// warning: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:17:10 ]
//     │
//  17 │ ╭─▶     _ => match (pet) {
//     ┆ ┆
//  21 │ ├─▶     }
//     │ │
//     │ ╰─────────── unreachable arm
//     │
//     │     Note: Error code: E0063
// ────╯
// error: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:20:12 ]
//     │
//  20 │       _ => "unreachable"
//     │            ──────┬──────
//     │                  ╰──────── unreachable arm
//     │
//     │ Note: Error code: E0063
// ────╯
// warning: unreachable arm
//     ╭─[ catch_exhaustiveness_checks.baml:27:10 ]
//     │
//  27 │ ╭─▶     _ => match (pet) {
//     ┆ ┆
//  29 │ ├─▶     }
//     │ │
//     │ ╰─────────── unreachable arm
//     │
//     │     Note: Error code: E0063
// ────╯
// error: non-exhaustive match on type Pet; missing: Dog { type: _ }
//     ╭─[ catch_exhaustiveness_checks.baml:27:10 ]
//     │
//  27 │ ╭─▶     _ => match (pet) {
//     ┆ ┆
//  29 │ ├─▶     }
//     │ │
//     │ ╰─────────── non-exhaustive match on type Pet; missing: Dog { type: _ }
//     │
//     │     Note: Error code: E0062
// ────╯
