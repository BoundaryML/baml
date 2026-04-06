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
// <no-diagnostics-expected>
