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
// E0063
//
//   × unreachable arm
//     ╭─[catch_exhaustiveness_checks.baml:20:12]
//  20 │       _ => "unreachable"
//     ·            ─────────────
//     ╰────
// E0062
//
//   × non-exhaustive match on type Cat | Dog; missing: Dog { type: _ }
//     ╭─[catch_exhaustiveness_checks.baml:27:10]
//  27 │ ╭─▶     _ => match (pet) {
//  28 │ │         let c: Cat => "cat"
//  29 │ ╰─▶     }
//     ╰────
