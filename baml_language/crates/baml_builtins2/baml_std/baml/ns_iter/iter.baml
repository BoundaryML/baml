class Done {}

interface Iterable {
  type Item
  type Error = never

  function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
}

interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
  type Item
  type Error = never

  function next(self) -> Self.Item | Done throws Self.Error

  function map<R, E2>(self, fn: (Self.Item) -> R throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    Map<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  function filter<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Filter<Self.Item, Self.Error, E2> { source: self, predicate: predicate }
  }

  function filter_map<R, E2>(self, fn: (Self.Item) -> R? throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    FilterMap<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  function flat_map<R, E2, E3>(self, fn: (Self.Item) -> Iterable<Item = R, Error = E3> throws E2) -> Iterator<Item = R, Error = Self.Error | E2 | E3> throws never {
    FlatMap<Self.Item, R, Self.Error, E2, E3> { source: self, func: fn, inner: Done {} }
  }

  function step_by(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    StepBy<Self.Item, Self.Error> { source: self, n: n, first: true }
  }

  function chain<E2>(self, other: Iterable<Item = Self.Item, Error = E2>) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Chain<Self.Item, Self.Error, E2> { first: self, other: other.iter(), first_done: false }
  }

  function peekable(self) -> Peekable<Self.Item, Self.Error> throws never {
    Peekable<Self.Item, Self.Error> { source: self, buffer: Done {}, has_buffer: false }
  }

  function collect(self) -> Self.Item[] throws Self.Error {
    let r: Self.Item[] = [];
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => r.push(x),
      }
    }
    r
  }

  function reduce<A, E2>(self, fn: (A, Self.Item) -> A throws E2, initial: A) -> A throws Self.Error | E2 {
    let acc: A = initial;
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => { acc = fn(acc, x); },
      }
    }
    acc
  }

  function count(self) -> int throws Self.Error {
    let n: int = 0;
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => { n += 1; },
      }
    }
    n
  }

  function for_each<E2>(self, fn: (Self.Item) -> void throws E2) -> void throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => { fn(x); },
      }
    }
  }

  function some<E2>(self, predicate: (Self.Item) -> bool throws E2) -> bool throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => {
          if (predicate(x)) {
            return true;
          }
        },
      }
    }
    false
  }

  function every<E2>(self, predicate: (Self.Item) -> bool throws E2) -> bool throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => {
          if (!predicate(x)) {
            return false;
          }
        },
      }
    }
    true
  }

  function find<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Self.Item? throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => {
          if (predicate(x)) {
            return x;
          }
        },
      }
    }
    null
  }
}

function flatten<R, E, E3, I extends Iterable<Item = R, Error = E3>>(src: Iterator<Item = I, Error = E>) -> Iterator<Item = R, Error = E | E3> throws never {
  Flatten<I, R, E, E3> { source: src, inner: Done {} }
}

class ArrayIterator<T> {
  arr: T[]
  idx: int

  function new(arr: T[]) -> ArrayIterator<T> throws never {
    ArrayIterator<T> { arr: arr, idx: 0 }
  }

  implements Iterable {
    type Item = T
    type Error = never

    function iter(self) -> Iterator<Item = T, Error = never> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = never

    function next(self) -> T | Done throws never {
      // Bounds-check by index, not by matching the element against null: when
      // T is itself nullable, a stored null element must be yielded, not
      // treated as the end of iteration.
      if (self.idx >= self.arr.length()) {
        return Done {};
      }
      // Index access yields the element at type `T` directly — in-bounds by
      // the check above. No `at()`-plus-match: gating every element on a
      // runtime `let value: T` test would cost a type test per element for a
      // read the bounds check has already proven safe.
      let item = self.arr[self.idx];
      self.idx += 1;
      return item;
    }
  }
}

class Range {
  i: int
  max: int
  step: int

  function new(min: int, max: int, step: int = 1) -> Range throws never {
    Range { i: min, max: max, step: step }
  }

  implements Iterable {
    type Item = int
    type Error = never

    function iter(self) -> Iterator<Item = int, Error = never> throws never { self }
  }

  implements Iterator {
    type Item = int
    type Error = never

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

class Repeat<T> {
  value: T
  count: int

  function new(value: T, count: int = -1) -> Repeat<T> throws never {
    Repeat<T> { value: value, count: count }
  }

  implements Iterable {
    type Item = T
    type Error = never

    function iter(self) -> Iterator<Item = T, Error = never> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = never

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

class Map<T, R, E, E2> {
  source: Iterator<Item = T, Error = E>
  func: (T) -> R throws E2

  implements Iterable {
    type Item = R
    type Error = E | E2

    function iter(self) -> Iterator<Item = R, Error = E | E2> throws never { self }
  }

  implements Iterator {
    type Item = R
    type Error = E | E2

    function next(self) -> R | Done throws E | E2 {
      match (self.source.next()) {
        Done => Done {},
        let x: T => self.func(x),
      }
    }
  }
}

class Filter<T, E, E2> {
  source: Iterator<Item = T, Error = E>
  predicate: (T) -> bool throws E2

  implements Iterable {
    type Item = T
    type Error = E | E2

    function iter(self) -> Iterator<Item = T, Error = E | E2> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = E | E2

    function next(self) -> T | Done throws E | E2 {
      while (true) {
        match (self.source.next()) {
          Done => { return Done {}; },
          let x: T => {
            if (self.predicate(x)) {
              return x;
            }
          },
        }
      }
      Done {}
    }
  }
}

class FilterMap<T, R, E, E2> {
  source: Iterator<Item = T, Error = E>
  func: (T) -> R? throws E2

  implements Iterable {
    type Item = R
    type Error = E | E2

    function iter(self) -> Iterator<Item = R, Error = E | E2> throws never { self }
  }

  implements Iterator {
    type Item = R
    type Error = E | E2

    function next(self) -> R | Done throws E | E2 {
      while (true) {
        match (self.source.next()) {
          Done => { return Done {}; },
          let x: T => {
            match (self.func(x)) {
              null => {},
              let y: R => { return y; },
            }
          },
        }
      }
      Done {}
    }
  }
}

class FlatMap<T, R, E, E2, E3> {
  source: Iterator<Item = T, Error = E>
  func: (T) -> Iterable<Item = R, Error = E3> throws E2
  inner: Iterator<Item = R, Error = E3> | Done

  implements Iterable {
    type Item = R
    type Error = E | E2 | E3

    function iter(self) -> Iterator<Item = R, Error = E | E2 | E3> throws never { self }
  }

  implements Iterator {
    type Item = R
    type Error = E | E2 | E3

    function next(self) -> R | Done throws E | E2 | E3 {
      while (true) {
        match (self.inner) {
          let cur: Iterator<Item = R, Error = E3> => {
            match (cur.next()) {
              Done => { self.inner = Done {}; },
              let y: R => { return y; },
            }
          },
          Done => {
            match (self.source.next()) {
              Done => { return Done {}; },
              let x: T => {
                let it = self.func(x).iter();
                self.inner = it;
              },
            }
          },
        }
      }
      Done {}
    }
  }
}

class Flatten<I extends Iterable<Item = R, Error = E3>, R, E, E3> {
  source: Iterator<Item = I, Error = E>
  inner: Iterator<Item = R, Error = E3> | Done

  implements Iterable {
    type Item = R
    type Error = E | E3

    function iter(self) -> Iterator<Item = R, Error = E | E3> throws never { self }
  }

  implements Iterator {
    type Item = R
    type Error = E | E3

    function next(self) -> R | Done throws E | E3 {
      while (true) {
        match (self.inner) {
          let cur: Iterator<Item = R, Error = E3> => {
            match (cur.next()) {
              Done => { self.inner = Done {}; },
              let y: R => { return y; },
            }
          },
          Done => {
            match (self.source.next()) {
              Done => { return Done {}; },
              let x: I => {
                let it = x.iter();
                self.inner = it;
              },
            }
          },
        }
      }
      Done {}
    }
  }
}

class Peekable<T, E> {
  source: Iterator<Item = T, Error = E>
  buffer: T | Done
  has_buffer: bool

  function peek(self) -> T | Done throws E {
    if (self.has_buffer) {
      return self.buffer;
    }
    let v = self.source.next();
    self.buffer = v;
    self.has_buffer = true;
    v
  }

  implements Iterable {
    type Item = T
    type Error = E

    function iter(self) -> Iterator<Item = T, Error = E> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = E

    function next(self) -> T | Done throws E {
      if (self.has_buffer) {
        let v = self.buffer;
        self.buffer = Done {};
        self.has_buffer = false;
        return v;
      }
      self.source.next()
    }
  }
}

class StepBy<T, E> {
  source: Iterator<Item = T, Error = E>
  n: int
  first: bool

  implements Iterable {
    type Item = T
    type Error = E

    function iter(self) -> Iterator<Item = T, Error = E> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = E

    function next(self) -> T | Done throws E {
      if (self.first) {
        self.first = false;
        return self.source.next();
      }
      for (let i = 0; i < self.n - 1; i += 1) {
        match (self.source.next()) {
          Done => { return Done {}; },
          let x: T => {},
        }
      }
      self.source.next()
    }
  }
}

class Chain<T, E, E2> {
  first: Iterator<Item = T, Error = E>
  other: Iterator<Item = T, Error = E2>
  first_done: bool

  implements Iterable {
    type Item = T
    type Error = E | E2

    function iter(self) -> Iterator<Item = T, Error = E | E2> throws never { self }
  }

  implements Iterator {
    type Item = T
    type Error = E | E2

    function next(self) -> T | Done throws E | E2 {
      if (!self.first_done) {
        match (self.first.next()) {
          Done => { self.first_done = true; },
          let x: T => { return x; },
        }
      }
      self.other.next()
    }
  }
}

implements<T> Iterable for T[] {
  type Item = T
  type Error = never

  function iter(self) -> Iterator<Item = T, Error = never> throws never {
    ArrayIterator.new(self)
  }
}

implements Iterable for string {
  type Item = string
  type Error = never

  function iter(self) -> Iterator<Item = string, Error = never> throws never {
    self.chars().iter()
  }
}
