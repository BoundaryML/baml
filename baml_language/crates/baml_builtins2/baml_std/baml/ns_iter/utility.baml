/// Concrete iterator sources: iterate arrays, integer ranges, and repeated
/// values. Also wires `T[]` into the protocol via an out-of-body `implements`,
/// so `[1, 2, 3].iter()` works.

/// Iterates the elements of an array in order. Takes a shallow copy of the
/// array at construction so later mutations of the source don't affect it.
class ArrayIterator<T> {
  arr: T[]
  idx: int

  /// Create an iterator over a snapshot of `arr`.
  function new(arr: T[]) -> ArrayIterator<T> throws never {
    ArrayIterator<T> { arr: arr.slice(0, arr.length()), idx: 0 }
  }

  implements Iterable<T, never> {
    function iter(self) -> Iterator<T, never> throws never { self }
  }
  implements Iterator<T, never> {
    function next(self) -> T | Done throws never {
      match (self.arr.at(self.idx)) {
        null => Done {},
        let item: T => {
          self.idx += 1;
          item
        },
      }
    }
  }
}

/// Iterates integers from `i` (inclusive) toward `max` (exclusive), advancing
/// by `step` each pull.
class Range {
  i: int
  max: int
  step: int

  /// Create a range `[min, max)` advancing by `step` (default `1`).
  function new(min: int, max: int, step: int = 1) -> Range throws never {
    Range { i: min, max: max, step: step }
  }

  implements Iterable<int, never> {
    function iter(self) -> Iterator<int, never> throws never { self }
  }
  implements Iterator<int, never> {
    function next(self) -> int | Done throws never {
      if (self.i < self.max) {
        let cur = self.i;
        self.i += self.step;
        cur
      } else {
        Done {}
      }
    }
  }
}

/// Repeats `value`. If `count < 0` the iterator is infinite; otherwise it
/// yields `value` exactly `count` times, then `Done`.
class Repeat<T> {
  value: T
  count: int

  /// Create a repeater of `value`. `count < 0` (the default) is infinite.
  function new(value: T, count: int = -1) -> Repeat<T> throws never {
    Repeat<T> { value: value, count: count }
  }

  implements Iterable<T, never> {
    function iter(self) -> Iterator<T, never> throws never { self }
  }
  implements Iterator<T, never> {
    function next(self) -> T | Done throws never {
      if (self.count < 0) {
        self.value
      } else if (self.count > 0) {
        self.count -= 1;
        self.value
      } else {
        Done {}
      }
    }
  }
}

/// Make every array iterable: `[1, 2, 3].iter()` yields an `ArrayIterator`.
implements<T> Iterable<T, never> for T[] {
  function iter(self) -> Iterator<T, never> throws never {
    ArrayIterator.new(self)
  }
}
