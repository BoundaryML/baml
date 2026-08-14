// Iterator protocol and concrete sources for the native BAML iterator tests.
// The tests live one namespace up, in `user.iter_impl_generics_only`, so they exercise qualified
// generic interface types across nested namespaces.

class Done {}

interface Iterable<T, E> {
  function iter(self) -> Iterator<T, E> throws never
}

interface Iterator<T, E> requires Iterable<T, E> {
  function next(self) -> T | Done throws E

  function map<R, E2>(self, fn: (T) -> R throws E2) -> Iterator<R, E | E2> throws never {
    Map<T, R, E, E2> { iter: self, func: fn }
  }

  function filter<E2>(self, predicate: (T) -> bool throws E2) -> Iterator<T, E | E2> throws never {
    Filter<T, E, E2> { iter: self, predicate: predicate }
  }

  function filter_map<R, E2>(self, fn: (T) -> R? throws E2) -> Iterator<R, E | E2> throws never {
    FilterMap<T, R, E, E2> { iter: self, func: fn }
  }

  function flat_map<R, E2, E3>(self, fn: (T) -> Iterable<R, E3> throws E2) -> Iterator<R, E | E2 | E3> throws never {
    FlatMap<T, R, E, E2, E3> { iter: self, func: fn, inner: Done {} }
  }

  function step_by(self, n: int) -> Iterator<T, E> throws never {
    StepBy<T, E> { iter: self, n: n, first: true }
  }

  function chain(self, other: Iterator<T, E>) -> Iterator<T, E> throws never {
    Chain<T, E> { iter: self, other: other, first_done: false }
  }

  function peekable(self) -> Peekable<T, E> throws never {
    Peekable<T, E> { iter: self, buffer: Done {}, has_buffer: false }
  }

  function collect(self) -> T[] throws E {
    let r: T[] = [];
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => r.push(x),
      }
    }
    r
  }

  function reduce<A, E2>(self, fn: (A, T) -> A throws E2, initial: A) -> A throws E | E2 {
    let acc: A = initial;
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => { acc = fn(acc, x); },
      }
    }
    acc
  }

  function count(self) -> int throws E {
    let n: int = 0;
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => { n += 1; },
      }
    }
    n
  }

  function some<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => {
          if (predicate(x)) {
            return true;
          }
        },
      }
    }
    false
  }

  function every<E2>(self, predicate: (T) -> bool throws E2) -> bool throws E | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => {
          if (!predicate(x)) {
            return false;
          }
        },
      }
    }
    true
  }

  function find<E2>(self, predicate: (T) -> bool throws E2) -> T? throws E | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: T => {
          if (predicate(x)) {
            return x;
          }
        },
      }
    }
    null
  }
}

function flatten<R, E, E3>(src: Iterator<Iterable<R, E3>, E>) -> Iterator<R, E | E3> throws never {
  Flatten<R, E, E3> { iter: src, inner: Done {} }
}

class ArrayIterator<T> {
  arr: T[]
  idx: int

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

class Range {
  i: int
  max: int
  step: int

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

class Repeat<T> {
  value: T
  count: int

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

interface OutOfBodyValue<T> {
  function get(self) -> T throws unknown
}

class OutOfBodyBox<T> {
  value: T

  function new(value: T) -> OutOfBodyBox<T> throws never {
    OutOfBodyBox<T> { value: value }
  }
}

implements<T> OutOfBodyValue<T> for OutOfBodyBox<T> {
  function get(self) -> T throws unknown {
    self.value
  }
}

class Map<T, R, E, E2> {
  iter: Iterator<T, E>
  func: (T) -> R throws E2

  implements Iterable<R, E | E2> {
    function iter(self) -> Iterator<R, E | E2> throws never { self }
  }

  implements Iterator<R, E | E2> {
    function next(self) -> R | Done throws E | E2 {
      match (self.iter.next()) {
        Done => Done {},
        let x: T => self.func(x),
      }
    }
  }
}

class Filter<T, E, E2> {
  iter: Iterator<T, E>
  predicate: (T) -> bool throws E2

  implements Iterable<T, E | E2> {
    function iter(self) -> Iterator<T, E | E2> throws never { self }
  }

  implements Iterator<T, E | E2> {
    function next(self) -> T | Done throws E | E2 {
      while (true) {
        match (self.iter.next()) {
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
  iter: Iterator<T, E>
  func: (T) -> R? throws E2

  implements Iterable<R, E | E2> {
    function iter(self) -> Iterator<R, E | E2> throws never { self }
  }

  implements Iterator<R, E | E2> {
    function next(self) -> R | Done throws E | E2 {
      while (true) {
        match (self.iter.next()) {
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
  iter: Iterator<T, E>
  func: (T) -> Iterable<R, E3> throws E2
  inner: Iterator<R, E3> | Done

  implements Iterable<R, E | E2 | E3> {
    function iter(self) -> Iterator<R, E | E2 | E3> throws never { self }
  }

  implements Iterator<R, E | E2 | E3> {
    function next(self) -> R | Done throws E | E2 | E3 {
      while (true) {
        match (self.inner) {
          let cur: Iterator<R, E3> => {
            match (cur.next()) {
              Done => { self.inner = Done {}; },
              let y: R => { return y; },
            }
          },
          Done => {
            match (self.iter.next()) {
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

class Flatten<R, E, E3> {
  iter: Iterator<Iterable<R, E3>, E>
  inner: Iterator<R, E3> | Done

  implements Iterable<R, E | E3> {
    function iter(self) -> Iterator<R, E | E3> throws never { self }
  }

  implements Iterator<R, E | E3> {
    function next(self) -> R | Done throws E | E3 {
      while (true) {
        match (self.inner) {
          let cur: Iterator<R, E3> => {
            match (cur.next()) {
              Done => { self.inner = Done {}; },
              let y: R => { return y; },
            }
          },
          Done => {
            match (self.iter.next()) {
              Done => { return Done {}; },
              let x: Iterable<R, E3> => {
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
  iter: Iterator<T, E>
  buffer: T | Done
  has_buffer: bool

  function peek(self) -> T | Done throws E {
    if (self.has_buffer) {
      return self.buffer;
    }
    let v = self.iter.next();
    self.buffer = v;
    self.has_buffer = true;
    v
  }

  implements Iterable<T, E> {
    function iter(self) -> Iterator<T, E> throws never { self }
  }

  implements Iterator<T, E> {
    function next(self) -> T | Done throws E {
      if (self.has_buffer) {
        let v = self.buffer;
        self.buffer = Done {};
        self.has_buffer = false;
        return v;
      }
      self.iter.next()
    }
  }
}

class StepBy<T, E> {
  iter: Iterator<T, E>
  n: int
  first: bool

  implements Iterable<T, E> {
    function iter(self) -> Iterator<T, E> throws never { self }
  }

  implements Iterator<T, E> {
    function next(self) -> T | Done throws E {
      if (self.first) {
        self.first = false;
        return self.iter.next();
      }
      for (let i = 0; i < self.n - 1; i += 1) {
        match (self.iter.next()) {
          Done => { return Done {}; },
          let x: T => {},
        }
      }
      self.iter.next()
    }
  }
}

class Chain<T, E> {
  iter: Iterator<T, E>
  other: Iterator<T, E>
  first_done: bool

  implements Iterable<T, E> {
    function iter(self) -> Iterator<T, E> throws never { self }
  }

  implements Iterator<T, E> {
    function next(self) -> T | Done throws E {
      if (!self.first_done) {
        match (self.iter.next()) {
          Done => { self.first_done = true; },
          let x: T => { return x; },
        }
      }
      self.other.next()
    }
  }
}
