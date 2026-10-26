// Iterator pipelines: an adapter chain over an array (filter, map, reduce)
// and a `for` loop over a class that implements the iterator protocol
// itself. 10 rounds over 2,000 numbers; about 30,000 elements pass through
// an iterator per job.
class Window {
  values: int[]
  pos: int
  end: int

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
      if (self.pos >= self.end) {
        return baml.iter.Done {};
      }
      let v = self.values[self.pos];
      self.pos += 1;
      v
    }
  }
}

function main() -> int {
  let xs: int[] = [];
  let seed = 5;
  for (let i = 0; i < 2000; i += 1) {
    seed = seed * 16807 % 2147483647;
    xs.push(seed % 1000);
  }
  let acc = 0;
  for (let round = 0; round < 10; round += 1) {
    let skip = round % 3;
    acc += xs.iter()
      .filter((x: int) -> bool { x % 3 != skip })
      .map((x: int) -> int { x * 2 + skip })
      .reduce((a: int, x: int) -> int { a + x }, 0);
    for (let v in Window { values: xs, pos: round * 100, end: round * 100 + 1000 }) {
      acc += v % 17;
    }
  }
  acc
}
