// Iterators as values: the adapters of `baml.iter.Iterator` take type
// arguments of their own and are called on whatever implements the
// interface, and the classes they return are implementations with type
// parameters.

class IterEvens {
  limit: int
  at: int

  implements baml.iter.Iterable {
    type Item = int
    type Error = never

    function iter(self) -> baml.iter.Iterator<Item = int, Error = never> throws never {
      self
    }
  }

  implements baml.iter.Iterator {
    type Item = int
    type Error = never

    function next(self) -> int | baml.iter.Done throws never {
      if (self.at >= self.limit) {
        return baml.iter.Done {};
      }
      let v = self.at;
      self.at += 2;
      v
    }
  }
}

function iter_adapter_chain() -> int {
  let skip = count(1);
  ints(20).iter()
    .filter((x: int) -> bool { x % 3 != skip })
    .map((x: int) -> int { x * 2 + skip })
    .reduce((a: int, x: int) -> int { a + x }, 0)
}
// An array's iterator that is more than a loop's position is an object.
function iter_array_iterator_as_a_value() -> int {
  let it = ints(6).iter();
  let first = it.next();
  let rest = it.reduce((a: int, x: int) -> int { a + x }, 0);
  match (first) {
    baml.iter.Done => -1,
    let x: int => rest * 100 + x,
  }
}
function iter_adapters_of_a_class() -> int {
  IterEvens { limit: count(11), at: 0 }
    .map((x: int) -> int { x + 1 })
    .reduce((a: int, x: int) -> int { a * 2 + x }, 0)
}
function iter_for_over_an_adapter() -> int {
  let n = 0;
  for (let v in IterEvens { limit: count(9), at: 0 }.filter((x: int) -> bool { x != 4 })) {
    n = n * 10 + v;
  }
  n
}
// Implementations whose items differ are behind different interfaces.
function iter_map_to_float() -> float {
  ints(6).iter()
    .map((x: int) -> float { x * 0.5 })
    .reduce((a: float, x: float) -> float { a + x }, 0.0)
}
function iter_map_to_string() -> string {
  ints(4).iter()
    .map((x: int) -> string { `<${x}>` })
    .reduce((a: string, s: string) -> string { a + s }, "")
}
// Items that are references share one copy of each adapter.
function iter_strings() -> string {
  words(4).iter()
    .filter((s: string) -> bool { s.length() != 2 })
    .map((s: string) -> string { s + "!" })
    .reduce((a: string, s: string) -> string { a + s }, "")
}
function iter_take_and_skip() -> int {
  ints(10).iter().take(4).skip(1).reduce((a: int, x: int) -> int { a + x }, 0)
}
