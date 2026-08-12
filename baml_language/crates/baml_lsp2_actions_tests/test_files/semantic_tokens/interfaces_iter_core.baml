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

//----
//- diagnostics
// E0097
//
//   ⚠ extraneous throws declaration: unknown
//      ╭─[interfaces_iter_core.baml:212:42]
//  212 │ ╭─▶   function get(self) -> T throws unknown {
//  213 │ │       self.value
//  214 │ ╰─▶   }
//      ╰────
//
//- semantic_tokens
// interfaces_iter_core.baml:1:1 (comment) len=77 "// Iterator protocol and concrete sources for the native BAML iterator tests."
// interfaces_iter_core.baml:2:1 (comment) len=97 "// The tests live one namespace up, in `user.iter_impl_generics_only`, so they exercise qualified"
// interfaces_iter_core.baml:3:1 (comment) len=52 "// generic interface types across nested namespaces."
// interfaces_iter_core.baml:5:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:5:7 (class) [declaration] len=4 "Done"
// interfaces_iter_core.baml:7:1 (keyword) len=9 "interface"
// interfaces_iter_core.baml:7:11 (interface) [declaration] len=8 "Iterable"
// interfaces_iter_core.baml:7:19 (operator) len=1 "<"
// interfaces_iter_core.baml:7:20 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:7:23 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:7:24 (operator) len=1 ">"
// interfaces_iter_core.baml:8:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:8:12 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:8:17 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:8:26 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:8:34 (operator) len=1 "<"
// interfaces_iter_core.baml:8:35 (type) len=1 "T"
// interfaces_iter_core.baml:8:38 (type) len=1 "E"
// interfaces_iter_core.baml:8:39 (operator) len=1 ">"
// interfaces_iter_core.baml:8:41 (keyword) len=6 "throws"
// interfaces_iter_core.baml:8:48 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:11:1 (keyword) len=9 "interface"
// interfaces_iter_core.baml:11:11 (interface) [declaration] len=8 "Iterator"
// interfaces_iter_core.baml:11:19 (operator) len=1 "<"
// interfaces_iter_core.baml:11:20 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:11:23 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:11:24 (operator) len=1 ">"
// interfaces_iter_core.baml:11:26 (keyword) len=8 "requires"
// interfaces_iter_core.baml:11:35 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:11:43 (operator) len=1 "<"
// interfaces_iter_core.baml:11:44 (type) len=1 "T"
// interfaces_iter_core.baml:11:47 (type) len=1 "E"
// interfaces_iter_core.baml:11:48 (operator) len=1 ">"
// interfaces_iter_core.baml:12:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:12:12 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:12:17 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:12:26 (type) len=1 "T"
// interfaces_iter_core.baml:12:28 (operator) len=1 "|"
// interfaces_iter_core.baml:12:30 (class) len=4 "Done"
// interfaces_iter_core.baml:12:35 (keyword) len=6 "throws"
// interfaces_iter_core.baml:12:42 (type) len=1 "E"
// interfaces_iter_core.baml:14:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:14:12 (method) [declaration] len=3 "map"
// interfaces_iter_core.baml:14:15 (operator) len=1 "<"
// interfaces_iter_core.baml:14:16 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:14:19 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:14:21 (operator) len=1 ">"
// interfaces_iter_core.baml:14:23 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:14:29 (parameter) [declaration] len=2 "fn"
// interfaces_iter_core.baml:14:34 (type) len=1 "T"
// interfaces_iter_core.baml:14:40 (type) len=1 "R"
// interfaces_iter_core.baml:14:42 (keyword) len=6 "throws"
// interfaces_iter_core.baml:14:49 (type) len=2 "E2"
// interfaces_iter_core.baml:14:56 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:14:64 (operator) len=1 "<"
// interfaces_iter_core.baml:14:65 (type) len=1 "R"
// interfaces_iter_core.baml:14:68 (type) len=1 "E"
// interfaces_iter_core.baml:14:70 (operator) len=1 "|"
// interfaces_iter_core.baml:14:72 (type) len=2 "E2"
// interfaces_iter_core.baml:14:74 (operator) len=1 ">"
// interfaces_iter_core.baml:14:76 (keyword) len=6 "throws"
// interfaces_iter_core.baml:14:83 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:15:5 (class) len=3 "Map"
// interfaces_iter_core.baml:15:8 (operator) len=1 "<"
// interfaces_iter_core.baml:15:9 (type) len=1 "T"
// interfaces_iter_core.baml:15:12 (type) len=1 "R"
// interfaces_iter_core.baml:15:15 (type) len=1 "E"
// interfaces_iter_core.baml:15:18 (type) len=2 "E2"
// interfaces_iter_core.baml:15:20 (operator) len=1 ">"
// interfaces_iter_core.baml:15:24 (property) len=4 "iter"
// interfaces_iter_core.baml:15:30 (parameter) len=4 "self"
// interfaces_iter_core.baml:15:36 (property) len=4 "func"
// interfaces_iter_core.baml:15:42 (parameter) len=2 "fn"
// interfaces_iter_core.baml:18:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:18:12 (method) [declaration] len=6 "filter"
// interfaces_iter_core.baml:18:18 (operator) len=1 "<"
// interfaces_iter_core.baml:18:19 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:18:21 (operator) len=1 ">"
// interfaces_iter_core.baml:18:23 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:18:29 (parameter) [declaration] len=9 "predicate"
// interfaces_iter_core.baml:18:41 (type) len=1 "T"
// interfaces_iter_core.baml:18:47 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:18:52 (keyword) len=6 "throws"
// interfaces_iter_core.baml:18:59 (type) len=2 "E2"
// interfaces_iter_core.baml:18:66 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:18:74 (operator) len=1 "<"
// interfaces_iter_core.baml:18:75 (type) len=1 "T"
// interfaces_iter_core.baml:18:78 (type) len=1 "E"
// interfaces_iter_core.baml:18:80 (operator) len=1 "|"
// interfaces_iter_core.baml:18:82 (type) len=2 "E2"
// interfaces_iter_core.baml:18:84 (operator) len=1 ">"
// interfaces_iter_core.baml:18:86 (keyword) len=6 "throws"
// interfaces_iter_core.baml:18:93 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:19:5 (class) len=6 "Filter"
// interfaces_iter_core.baml:19:11 (operator) len=1 "<"
// interfaces_iter_core.baml:19:12 (type) len=1 "T"
// interfaces_iter_core.baml:19:15 (type) len=1 "E"
// interfaces_iter_core.baml:19:18 (type) len=2 "E2"
// interfaces_iter_core.baml:19:20 (operator) len=1 ">"
// interfaces_iter_core.baml:19:24 (property) len=4 "iter"
// interfaces_iter_core.baml:19:30 (parameter) len=4 "self"
// interfaces_iter_core.baml:19:36 (property) len=9 "predicate"
// interfaces_iter_core.baml:19:47 (parameter) len=9 "predicate"
// interfaces_iter_core.baml:22:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:22:12 (method) [declaration] len=10 "filter_map"
// interfaces_iter_core.baml:22:22 (operator) len=1 "<"
// interfaces_iter_core.baml:22:23 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:22:26 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:22:28 (operator) len=1 ">"
// interfaces_iter_core.baml:22:30 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:22:36 (parameter) [declaration] len=2 "fn"
// interfaces_iter_core.baml:22:41 (type) len=1 "T"
// interfaces_iter_core.baml:22:47 (type) len=1 "R"
// interfaces_iter_core.baml:22:50 (keyword) len=6 "throws"
// interfaces_iter_core.baml:22:57 (type) len=2 "E2"
// interfaces_iter_core.baml:22:64 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:22:72 (operator) len=1 "<"
// interfaces_iter_core.baml:22:73 (type) len=1 "R"
// interfaces_iter_core.baml:22:76 (type) len=1 "E"
// interfaces_iter_core.baml:22:78 (operator) len=1 "|"
// interfaces_iter_core.baml:22:80 (type) len=2 "E2"
// interfaces_iter_core.baml:22:82 (operator) len=1 ">"
// interfaces_iter_core.baml:22:84 (keyword) len=6 "throws"
// interfaces_iter_core.baml:22:91 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:23:5 (class) len=9 "FilterMap"
// interfaces_iter_core.baml:23:14 (operator) len=1 "<"
// interfaces_iter_core.baml:23:15 (type) len=1 "T"
// interfaces_iter_core.baml:23:18 (type) len=1 "R"
// interfaces_iter_core.baml:23:21 (type) len=1 "E"
// interfaces_iter_core.baml:23:24 (type) len=2 "E2"
// interfaces_iter_core.baml:23:26 (operator) len=1 ">"
// interfaces_iter_core.baml:23:30 (property) len=4 "iter"
// interfaces_iter_core.baml:23:36 (parameter) len=4 "self"
// interfaces_iter_core.baml:23:42 (property) len=4 "func"
// interfaces_iter_core.baml:23:48 (parameter) len=2 "fn"
// interfaces_iter_core.baml:26:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:26:12 (method) [declaration] len=8 "flat_map"
// interfaces_iter_core.baml:26:20 (operator) len=1 "<"
// interfaces_iter_core.baml:26:21 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:26:24 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:26:28 (typeParameter) [declaration] len=2 "E3"
// interfaces_iter_core.baml:26:30 (operator) len=1 ">"
// interfaces_iter_core.baml:26:32 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:26:38 (parameter) [declaration] len=2 "fn"
// interfaces_iter_core.baml:26:43 (type) len=1 "T"
// interfaces_iter_core.baml:26:49 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:26:57 (operator) len=1 "<"
// interfaces_iter_core.baml:26:58 (type) len=1 "R"
// interfaces_iter_core.baml:26:61 (type) len=2 "E3"
// interfaces_iter_core.baml:26:63 (operator) len=1 ">"
// interfaces_iter_core.baml:26:65 (keyword) len=6 "throws"
// interfaces_iter_core.baml:26:72 (type) len=2 "E2"
// interfaces_iter_core.baml:26:79 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:26:87 (operator) len=1 "<"
// interfaces_iter_core.baml:26:88 (type) len=1 "R"
// interfaces_iter_core.baml:26:91 (type) len=1 "E"
// interfaces_iter_core.baml:26:93 (operator) len=1 "|"
// interfaces_iter_core.baml:26:95 (type) len=2 "E2"
// interfaces_iter_core.baml:26:98 (operator) len=1 "|"
// interfaces_iter_core.baml:26:100 (type) len=2 "E3"
// interfaces_iter_core.baml:26:102 (operator) len=1 ">"
// interfaces_iter_core.baml:26:104 (keyword) len=6 "throws"
// interfaces_iter_core.baml:26:111 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:27:5 (class) len=7 "FlatMap"
// interfaces_iter_core.baml:27:12 (operator) len=1 "<"
// interfaces_iter_core.baml:27:13 (type) len=1 "T"
// interfaces_iter_core.baml:27:16 (type) len=1 "R"
// interfaces_iter_core.baml:27:19 (type) len=1 "E"
// interfaces_iter_core.baml:27:22 (type) len=2 "E2"
// interfaces_iter_core.baml:27:26 (type) len=2 "E3"
// interfaces_iter_core.baml:27:28 (operator) len=1 ">"
// interfaces_iter_core.baml:27:32 (property) len=4 "iter"
// interfaces_iter_core.baml:27:38 (parameter) len=4 "self"
// interfaces_iter_core.baml:27:44 (property) len=4 "func"
// interfaces_iter_core.baml:27:50 (parameter) len=2 "fn"
// interfaces_iter_core.baml:27:54 (property) len=5 "inner"
// interfaces_iter_core.baml:27:61 (class) len=4 "Done"
// interfaces_iter_core.baml:30:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:30:12 (method) [declaration] len=7 "step_by"
// interfaces_iter_core.baml:30:20 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:30:26 (parameter) [declaration] len=1 "n"
// interfaces_iter_core.baml:30:29 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:30:37 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:30:45 (operator) len=1 "<"
// interfaces_iter_core.baml:30:46 (type) len=1 "T"
// interfaces_iter_core.baml:30:49 (type) len=1 "E"
// interfaces_iter_core.baml:30:50 (operator) len=1 ">"
// interfaces_iter_core.baml:30:52 (keyword) len=6 "throws"
// interfaces_iter_core.baml:30:59 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:31:5 (class) len=6 "StepBy"
// interfaces_iter_core.baml:31:11 (operator) len=1 "<"
// interfaces_iter_core.baml:31:12 (type) len=1 "T"
// interfaces_iter_core.baml:31:15 (type) len=1 "E"
// interfaces_iter_core.baml:31:16 (operator) len=1 ">"
// interfaces_iter_core.baml:31:20 (property) len=4 "iter"
// interfaces_iter_core.baml:31:26 (parameter) len=4 "self"
// interfaces_iter_core.baml:31:32 (property) len=1 "n"
// interfaces_iter_core.baml:31:35 (parameter) len=1 "n"
// interfaces_iter_core.baml:31:38 (property) len=5 "first"
// interfaces_iter_core.baml:31:45 (boolean) len=4 "true"
// interfaces_iter_core.baml:34:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:34:12 (method) [declaration] len=5 "chain"
// interfaces_iter_core.baml:34:18 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:34:24 (parameter) [declaration] len=5 "other"
// interfaces_iter_core.baml:34:31 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:34:39 (operator) len=1 "<"
// interfaces_iter_core.baml:34:40 (type) len=1 "T"
// interfaces_iter_core.baml:34:43 (type) len=1 "E"
// interfaces_iter_core.baml:34:44 (operator) len=1 ">"
// interfaces_iter_core.baml:34:50 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:34:58 (operator) len=1 "<"
// interfaces_iter_core.baml:34:59 (type) len=1 "T"
// interfaces_iter_core.baml:34:62 (type) len=1 "E"
// interfaces_iter_core.baml:34:63 (operator) len=1 ">"
// interfaces_iter_core.baml:34:65 (keyword) len=6 "throws"
// interfaces_iter_core.baml:34:72 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:35:5 (class) len=5 "Chain"
// interfaces_iter_core.baml:35:10 (operator) len=1 "<"
// interfaces_iter_core.baml:35:11 (type) len=1 "T"
// interfaces_iter_core.baml:35:14 (type) len=1 "E"
// interfaces_iter_core.baml:35:15 (operator) len=1 ">"
// interfaces_iter_core.baml:35:19 (property) len=4 "iter"
// interfaces_iter_core.baml:35:25 (parameter) len=4 "self"
// interfaces_iter_core.baml:35:31 (property) len=5 "other"
// interfaces_iter_core.baml:35:38 (parameter) len=5 "other"
// interfaces_iter_core.baml:35:45 (property) len=10 "first_done"
// interfaces_iter_core.baml:35:57 (boolean) len=5 "false"
// interfaces_iter_core.baml:38:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:38:12 (method) [declaration] len=8 "peekable"
// interfaces_iter_core.baml:38:21 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:38:30 (class) len=8 "Peekable"
// interfaces_iter_core.baml:38:38 (operator) len=1 "<"
// interfaces_iter_core.baml:38:39 (type) len=1 "T"
// interfaces_iter_core.baml:38:42 (type) len=1 "E"
// interfaces_iter_core.baml:38:43 (operator) len=1 ">"
// interfaces_iter_core.baml:38:45 (keyword) len=6 "throws"
// interfaces_iter_core.baml:38:52 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:39:5 (class) len=8 "Peekable"
// interfaces_iter_core.baml:39:13 (operator) len=1 "<"
// interfaces_iter_core.baml:39:14 (type) len=1 "T"
// interfaces_iter_core.baml:39:17 (type) len=1 "E"
// interfaces_iter_core.baml:39:18 (operator) len=1 ">"
// interfaces_iter_core.baml:39:22 (property) len=4 "iter"
// interfaces_iter_core.baml:39:28 (parameter) len=4 "self"
// interfaces_iter_core.baml:39:34 (property) len=6 "buffer"
// interfaces_iter_core.baml:39:42 (class) len=4 "Done"
// interfaces_iter_core.baml:39:51 (property) len=10 "has_buffer"
// interfaces_iter_core.baml:39:63 (boolean) len=5 "false"
// interfaces_iter_core.baml:42:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:42:12 (method) [declaration] len=7 "collect"
// interfaces_iter_core.baml:42:20 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:42:29 (type) len=1 "T"
// interfaces_iter_core.baml:42:33 (keyword) len=6 "throws"
// interfaces_iter_core.baml:42:40 (type) len=1 "E"
// interfaces_iter_core.baml:43:5 (keyword) len=3 "let"
// interfaces_iter_core.baml:43:9 (variable) [declaration] len=1 "r"
// interfaces_iter_core.baml:43:12 (type) len=1 "T"
// interfaces_iter_core.baml:43:16 (operator) len=1 "="
// interfaces_iter_core.baml:44:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:44:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:45:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:45:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:45:19 (method) len=4 "next"
// interfaces_iter_core.baml:46:9 (class) len=4 "Done"
// interfaces_iter_core.baml:46:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:47:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:47:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:47:16 (type) len=1 "T"
// interfaces_iter_core.baml:47:21 (variable) len=1 "r"
// interfaces_iter_core.baml:47:23 (method) len=4 "push"
// interfaces_iter_core.baml:47:28 (variable) len=1 "x"
// interfaces_iter_core.baml:50:5 (variable) len=1 "r"
// interfaces_iter_core.baml:53:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:53:12 (method) [declaration] len=6 "reduce"
// interfaces_iter_core.baml:53:18 (operator) len=1 "<"
// interfaces_iter_core.baml:53:19 (typeParameter) [declaration] len=1 "A"
// interfaces_iter_core.baml:53:22 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:53:24 (operator) len=1 ">"
// interfaces_iter_core.baml:53:26 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:53:32 (parameter) [declaration] len=2 "fn"
// interfaces_iter_core.baml:53:37 (type) len=1 "A"
// interfaces_iter_core.baml:53:40 (type) len=1 "T"
// interfaces_iter_core.baml:53:46 (type) len=1 "A"
// interfaces_iter_core.baml:53:48 (keyword) len=6 "throws"
// interfaces_iter_core.baml:53:55 (type) len=2 "E2"
// interfaces_iter_core.baml:53:59 (parameter) [declaration] len=7 "initial"
// interfaces_iter_core.baml:53:68 (type) len=1 "A"
// interfaces_iter_core.baml:53:74 (type) len=1 "A"
// interfaces_iter_core.baml:53:76 (keyword) len=6 "throws"
// interfaces_iter_core.baml:53:83 (type) len=1 "E"
// interfaces_iter_core.baml:53:85 (operator) len=1 "|"
// interfaces_iter_core.baml:53:87 (type) len=2 "E2"
// interfaces_iter_core.baml:54:5 (keyword) len=3 "let"
// interfaces_iter_core.baml:54:9 (variable) [declaration] len=3 "acc"
// interfaces_iter_core.baml:54:14 (type) len=1 "A"
// interfaces_iter_core.baml:54:16 (operator) len=1 "="
// interfaces_iter_core.baml:54:18 (parameter) len=7 "initial"
// interfaces_iter_core.baml:55:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:55:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:56:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:56:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:56:19 (method) len=4 "next"
// interfaces_iter_core.baml:57:9 (class) len=4 "Done"
// interfaces_iter_core.baml:57:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:58:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:58:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:58:16 (type) len=1 "T"
// interfaces_iter_core.baml:58:23 (variable) len=3 "acc"
// interfaces_iter_core.baml:58:27 (operator) len=1 "="
// interfaces_iter_core.baml:58:29 (parameter) len=2 "fn"
// interfaces_iter_core.baml:58:32 (variable) len=3 "acc"
// interfaces_iter_core.baml:58:37 (variable) len=1 "x"
// interfaces_iter_core.baml:61:5 (variable) len=3 "acc"
// interfaces_iter_core.baml:64:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:64:12 (method) [declaration] len=5 "count"
// interfaces_iter_core.baml:64:18 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:64:27 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:64:31 (keyword) len=6 "throws"
// interfaces_iter_core.baml:64:38 (type) len=1 "E"
// interfaces_iter_core.baml:65:5 (keyword) len=3 "let"
// interfaces_iter_core.baml:65:9 (variable) [declaration] len=1 "n"
// interfaces_iter_core.baml:65:12 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:65:16 (operator) len=1 "="
// interfaces_iter_core.baml:65:18 (number) len=1 "0"
// interfaces_iter_core.baml:66:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:66:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:67:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:67:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:67:19 (method) len=4 "next"
// interfaces_iter_core.baml:68:9 (class) len=4 "Done"
// interfaces_iter_core.baml:68:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:69:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:69:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:69:16 (type) len=1 "T"
// interfaces_iter_core.baml:69:23 (variable) len=1 "n"
// interfaces_iter_core.baml:69:25 (operator) len=2 "+="
// interfaces_iter_core.baml:69:28 (number) len=1 "1"
// interfaces_iter_core.baml:72:5 (variable) len=1 "n"
// interfaces_iter_core.baml:75:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:75:12 (method) [declaration] len=4 "some"
// interfaces_iter_core.baml:75:16 (operator) len=1 "<"
// interfaces_iter_core.baml:75:17 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:75:19 (operator) len=1 ">"
// interfaces_iter_core.baml:75:21 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:75:27 (parameter) [declaration] len=9 "predicate"
// interfaces_iter_core.baml:75:39 (type) len=1 "T"
// interfaces_iter_core.baml:75:45 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:75:50 (keyword) len=6 "throws"
// interfaces_iter_core.baml:75:57 (type) len=2 "E2"
// interfaces_iter_core.baml:75:64 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:75:69 (keyword) len=6 "throws"
// interfaces_iter_core.baml:75:76 (type) len=1 "E"
// interfaces_iter_core.baml:75:78 (operator) len=1 "|"
// interfaces_iter_core.baml:75:80 (type) len=2 "E2"
// interfaces_iter_core.baml:76:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:76:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:77:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:77:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:77:19 (method) len=4 "next"
// interfaces_iter_core.baml:78:9 (class) len=4 "Done"
// interfaces_iter_core.baml:78:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:79:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:79:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:79:16 (type) len=1 "T"
// interfaces_iter_core.baml:80:11 (keyword) len=2 "if"
// interfaces_iter_core.baml:80:15 (parameter) len=9 "predicate"
// interfaces_iter_core.baml:80:25 (variable) len=1 "x"
// interfaces_iter_core.baml:81:13 (keyword) len=6 "return"
// interfaces_iter_core.baml:81:20 (boolean) len=4 "true"
// interfaces_iter_core.baml:86:5 (boolean) len=5 "false"
// interfaces_iter_core.baml:89:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:89:12 (method) [declaration] len=5 "every"
// interfaces_iter_core.baml:89:17 (operator) len=1 "<"
// interfaces_iter_core.baml:89:18 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:89:20 (operator) len=1 ">"
// interfaces_iter_core.baml:89:22 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:89:28 (parameter) [declaration] len=9 "predicate"
// interfaces_iter_core.baml:89:40 (type) len=1 "T"
// interfaces_iter_core.baml:89:46 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:89:51 (keyword) len=6 "throws"
// interfaces_iter_core.baml:89:58 (type) len=2 "E2"
// interfaces_iter_core.baml:89:65 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:89:70 (keyword) len=6 "throws"
// interfaces_iter_core.baml:89:77 (type) len=1 "E"
// interfaces_iter_core.baml:89:79 (operator) len=1 "|"
// interfaces_iter_core.baml:89:81 (type) len=2 "E2"
// interfaces_iter_core.baml:90:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:90:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:91:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:91:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:91:19 (method) len=4 "next"
// interfaces_iter_core.baml:92:9 (class) len=4 "Done"
// interfaces_iter_core.baml:92:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:93:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:93:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:93:16 (type) len=1 "T"
// interfaces_iter_core.baml:94:11 (keyword) len=2 "if"
// interfaces_iter_core.baml:94:15 (operator) len=1 "!"
// interfaces_iter_core.baml:94:16 (parameter) len=9 "predicate"
// interfaces_iter_core.baml:94:26 (variable) len=1 "x"
// interfaces_iter_core.baml:95:13 (keyword) len=6 "return"
// interfaces_iter_core.baml:95:20 (boolean) len=5 "false"
// interfaces_iter_core.baml:100:5 (boolean) len=4 "true"
// interfaces_iter_core.baml:103:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:103:12 (method) [declaration] len=4 "find"
// interfaces_iter_core.baml:103:16 (operator) len=1 "<"
// interfaces_iter_core.baml:103:17 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:103:19 (operator) len=1 ">"
// interfaces_iter_core.baml:103:21 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:103:27 (parameter) [declaration] len=9 "predicate"
// interfaces_iter_core.baml:103:39 (type) len=1 "T"
// interfaces_iter_core.baml:103:45 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:103:50 (keyword) len=6 "throws"
// interfaces_iter_core.baml:103:57 (type) len=2 "E2"
// interfaces_iter_core.baml:103:64 (type) len=1 "T"
// interfaces_iter_core.baml:103:67 (keyword) len=6 "throws"
// interfaces_iter_core.baml:103:74 (type) len=1 "E"
// interfaces_iter_core.baml:103:76 (operator) len=1 "|"
// interfaces_iter_core.baml:103:78 (type) len=2 "E2"
// interfaces_iter_core.baml:104:5 (keyword) len=5 "while"
// interfaces_iter_core.baml:104:12 (boolean) len=4 "true"
// interfaces_iter_core.baml:105:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:105:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:105:19 (method) len=4 "next"
// interfaces_iter_core.baml:106:9 (class) len=4 "Done"
// interfaces_iter_core.baml:106:19 (keyword) len=5 "break"
// interfaces_iter_core.baml:107:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:107:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:107:16 (type) len=1 "T"
// interfaces_iter_core.baml:108:11 (keyword) len=2 "if"
// interfaces_iter_core.baml:108:15 (parameter) len=9 "predicate"
// interfaces_iter_core.baml:108:25 (variable) len=1 "x"
// interfaces_iter_core.baml:109:13 (keyword) len=6 "return"
// interfaces_iter_core.baml:109:20 (variable) len=1 "x"
// interfaces_iter_core.baml:114:5 (type) [defaultLibrary] len=4 "null"
// interfaces_iter_core.baml:118:1 (keyword) len=8 "function"
// interfaces_iter_core.baml:118:10 (function) [declaration] len=7 "flatten"
// interfaces_iter_core.baml:118:17 (operator) len=1 "<"
// interfaces_iter_core.baml:118:18 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:118:21 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:118:24 (typeParameter) [declaration] len=2 "E3"
// interfaces_iter_core.baml:118:26 (operator) len=1 ">"
// interfaces_iter_core.baml:118:28 (parameter) [declaration] len=3 "src"
// interfaces_iter_core.baml:118:33 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:118:41 (operator) len=1 "<"
// interfaces_iter_core.baml:118:42 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:118:50 (operator) len=1 "<"
// interfaces_iter_core.baml:118:51 (type) len=1 "R"
// interfaces_iter_core.baml:118:54 (type) len=2 "E3"
// interfaces_iter_core.baml:118:56 (operator) len=1 ">"
// interfaces_iter_core.baml:118:59 (type) len=1 "E"
// interfaces_iter_core.baml:118:60 (operator) len=1 ">"
// interfaces_iter_core.baml:118:66 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:118:74 (operator) len=1 "<"
// interfaces_iter_core.baml:118:75 (type) len=1 "R"
// interfaces_iter_core.baml:118:78 (type) len=1 "E"
// interfaces_iter_core.baml:118:80 (operator) len=1 "|"
// interfaces_iter_core.baml:118:82 (type) len=2 "E3"
// interfaces_iter_core.baml:118:84 (operator) len=1 ">"
// interfaces_iter_core.baml:118:86 (keyword) len=6 "throws"
// interfaces_iter_core.baml:118:93 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:119:3 (class) len=7 "Flatten"
// interfaces_iter_core.baml:119:10 (operator) len=1 "<"
// interfaces_iter_core.baml:119:11 (type) len=1 "R"
// interfaces_iter_core.baml:119:14 (type) len=1 "E"
// interfaces_iter_core.baml:119:17 (type) len=2 "E3"
// interfaces_iter_core.baml:119:19 (operator) len=1 ">"
// interfaces_iter_core.baml:119:23 (property) len=4 "iter"
// interfaces_iter_core.baml:119:29 (parameter) len=3 "src"
// interfaces_iter_core.baml:119:34 (property) len=5 "inner"
// interfaces_iter_core.baml:119:41 (class) len=4 "Done"
// interfaces_iter_core.baml:122:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:122:7 (class) [declaration] len=13 "ArrayIterator"
// interfaces_iter_core.baml:122:20 (operator) len=1 "<"
// interfaces_iter_core.baml:122:21 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:122:22 (operator) len=1 ">"
// interfaces_iter_core.baml:123:3 (property) [declaration] len=3 "arr"
// interfaces_iter_core.baml:123:8 (type) len=1 "T"
// interfaces_iter_core.baml:124:3 (property) [declaration] len=3 "idx"
// interfaces_iter_core.baml:124:8 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:126:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:126:12 (method) [declaration] len=3 "new"
// interfaces_iter_core.baml:126:16 (parameter) [declaration] len=3 "arr"
// interfaces_iter_core.baml:126:21 (type) len=1 "T"
// interfaces_iter_core.baml:126:29 (class) len=13 "ArrayIterator"
// interfaces_iter_core.baml:126:42 (operator) len=1 "<"
// interfaces_iter_core.baml:126:43 (type) len=1 "T"
// interfaces_iter_core.baml:126:44 (operator) len=1 ">"
// interfaces_iter_core.baml:126:46 (keyword) len=6 "throws"
// interfaces_iter_core.baml:126:53 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:127:5 (class) len=13 "ArrayIterator"
// interfaces_iter_core.baml:127:18 (operator) len=1 "<"
// interfaces_iter_core.baml:127:19 (type) len=1 "T"
// interfaces_iter_core.baml:127:20 (operator) len=1 ">"
// interfaces_iter_core.baml:127:24 (property) len=3 "arr"
// interfaces_iter_core.baml:127:29 (parameter) len=3 "arr"
// interfaces_iter_core.baml:127:33 (method) len=5 "slice"
// interfaces_iter_core.baml:127:39 (number) len=1 "0"
// interfaces_iter_core.baml:127:42 (parameter) len=3 "arr"
// interfaces_iter_core.baml:127:46 (method) len=6 "length"
// interfaces_iter_core.baml:127:57 (property) len=3 "idx"
// interfaces_iter_core.baml:127:62 (number) len=1 "0"
// interfaces_iter_core.baml:130:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:130:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:130:22 (operator) len=1 "<"
// interfaces_iter_core.baml:130:23 (type) len=1 "T"
// interfaces_iter_core.baml:130:26 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:130:31 (operator) len=1 ">"
// interfaces_iter_core.baml:131:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:131:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:131:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:131:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:131:36 (operator) len=1 "<"
// interfaces_iter_core.baml:131:37 (type) len=1 "T"
// interfaces_iter_core.baml:131:40 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:131:45 (operator) len=1 ">"
// interfaces_iter_core.baml:131:47 (keyword) len=6 "throws"
// interfaces_iter_core.baml:131:54 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:131:62 (parameter) len=4 "self"
// interfaces_iter_core.baml:134:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:134:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:134:22 (operator) len=1 "<"
// interfaces_iter_core.baml:134:23 (type) len=1 "T"
// interfaces_iter_core.baml:134:26 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:134:31 (operator) len=1 ">"
// interfaces_iter_core.baml:135:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:135:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:135:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:135:28 (type) len=1 "T"
// interfaces_iter_core.baml:135:30 (operator) len=1 "|"
// interfaces_iter_core.baml:135:32 (class) len=4 "Done"
// interfaces_iter_core.baml:135:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:135:44 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:136:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:136:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:136:19 (property) len=3 "arr"
// interfaces_iter_core.baml:136:23 (method) len=2 "at"
// interfaces_iter_core.baml:136:26 (parameter) len=4 "self"
// interfaces_iter_core.baml:136:31 (property) len=3 "idx"
// interfaces_iter_core.baml:137:9 (type) [defaultLibrary] len=4 "null"
// interfaces_iter_core.baml:137:17 (class) len=4 "Done"
// interfaces_iter_core.baml:138:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:138:13 (variable) [declaration] len=4 "item"
// interfaces_iter_core.baml:138:19 (type) len=1 "T"
// interfaces_iter_core.baml:139:11 (parameter) len=4 "self"
// interfaces_iter_core.baml:139:16 (property) len=3 "idx"
// interfaces_iter_core.baml:139:20 (operator) len=2 "+="
// interfaces_iter_core.baml:139:23 (number) len=1 "1"
// interfaces_iter_core.baml:140:11 (variable) len=4 "item"
// interfaces_iter_core.baml:147:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:147:7 (class) [declaration] len=5 "Range"
// interfaces_iter_core.baml:148:3 (property) [declaration] len=1 "i"
// interfaces_iter_core.baml:148:6 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:149:3 (property) [declaration] len=3 "max"
// interfaces_iter_core.baml:149:8 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:150:3 (property) [declaration] len=4 "step"
// interfaces_iter_core.baml:150:9 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:152:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:152:12 (method) [declaration] len=3 "new"
// interfaces_iter_core.baml:152:16 (parameter) [declaration] len=3 "min"
// interfaces_iter_core.baml:152:21 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:152:26 (parameter) [declaration] len=3 "max"
// interfaces_iter_core.baml:152:31 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:152:36 (parameter) [declaration] len=4 "step"
// interfaces_iter_core.baml:152:42 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:152:46 (operator) len=1 "="
// interfaces_iter_core.baml:152:48 (number) len=1 "1"
// interfaces_iter_core.baml:152:54 (class) len=5 "Range"
// interfaces_iter_core.baml:152:60 (keyword) len=6 "throws"
// interfaces_iter_core.baml:152:67 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:153:5 (class) len=5 "Range"
// interfaces_iter_core.baml:153:13 (property) len=1 "i"
// interfaces_iter_core.baml:153:16 (parameter) len=3 "min"
// interfaces_iter_core.baml:153:21 (property) len=3 "max"
// interfaces_iter_core.baml:153:26 (parameter) len=3 "max"
// interfaces_iter_core.baml:153:31 (property) len=4 "step"
// interfaces_iter_core.baml:153:37 (parameter) len=4 "step"
// interfaces_iter_core.baml:156:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:156:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:156:22 (operator) len=1 "<"
// interfaces_iter_core.baml:156:23 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:156:28 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:156:33 (operator) len=1 ">"
// interfaces_iter_core.baml:157:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:157:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:157:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:157:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:157:36 (operator) len=1 "<"
// interfaces_iter_core.baml:157:37 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:157:42 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:157:47 (operator) len=1 ">"
// interfaces_iter_core.baml:157:49 (keyword) len=6 "throws"
// interfaces_iter_core.baml:157:56 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:157:64 (parameter) len=4 "self"
// interfaces_iter_core.baml:160:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:160:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:160:22 (operator) len=1 "<"
// interfaces_iter_core.baml:160:23 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:160:28 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:160:33 (operator) len=1 ">"
// interfaces_iter_core.baml:161:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:161:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:161:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:161:28 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:161:32 (operator) len=1 "|"
// interfaces_iter_core.baml:161:34 (class) len=4 "Done"
// interfaces_iter_core.baml:161:39 (keyword) len=6 "throws"
// interfaces_iter_core.baml:161:46 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:162:7 (keyword) len=2 "if"
// interfaces_iter_core.baml:162:11 (parameter) len=4 "self"
// interfaces_iter_core.baml:162:16 (property) len=1 "i"
// interfaces_iter_core.baml:162:18 (operator) len=1 "<"
// interfaces_iter_core.baml:162:20 (parameter) len=4 "self"
// interfaces_iter_core.baml:162:25 (property) len=3 "max"
// interfaces_iter_core.baml:163:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:163:13 (variable) [declaration] len=3 "cur"
// interfaces_iter_core.baml:163:17 (operator) len=1 "="
// interfaces_iter_core.baml:163:19 (parameter) len=4 "self"
// interfaces_iter_core.baml:163:24 (property) len=1 "i"
// interfaces_iter_core.baml:164:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:164:14 (property) len=1 "i"
// interfaces_iter_core.baml:164:16 (operator) len=2 "+="
// interfaces_iter_core.baml:164:19 (parameter) len=4 "self"
// interfaces_iter_core.baml:164:24 (property) len=4 "step"
// interfaces_iter_core.baml:165:9 (variable) len=3 "cur"
// interfaces_iter_core.baml:166:9 (keyword) len=4 "else"
// interfaces_iter_core.baml:167:9 (class) len=4 "Done"
// interfaces_iter_core.baml:173:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:173:7 (class) [declaration] len=6 "Repeat"
// interfaces_iter_core.baml:173:13 (operator) len=1 "<"
// interfaces_iter_core.baml:173:14 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:173:15 (operator) len=1 ">"
// interfaces_iter_core.baml:174:3 (property) [declaration] len=5 "value"
// interfaces_iter_core.baml:174:10 (type) len=1 "T"
// interfaces_iter_core.baml:175:3 (property) [declaration] len=5 "count"
// interfaces_iter_core.baml:175:10 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:177:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:177:12 (method) [declaration] len=3 "new"
// interfaces_iter_core.baml:177:16 (parameter) [declaration] len=5 "value"
// interfaces_iter_core.baml:177:23 (type) len=1 "T"
// interfaces_iter_core.baml:177:26 (parameter) [declaration] len=5 "count"
// interfaces_iter_core.baml:177:33 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:177:37 (operator) len=1 "="
// interfaces_iter_core.baml:177:39 (operator) len=1 "-"
// interfaces_iter_core.baml:177:40 (number) len=1 "1"
// interfaces_iter_core.baml:177:46 (class) len=6 "Repeat"
// interfaces_iter_core.baml:177:52 (operator) len=1 "<"
// interfaces_iter_core.baml:177:53 (type) len=1 "T"
// interfaces_iter_core.baml:177:54 (operator) len=1 ">"
// interfaces_iter_core.baml:177:56 (keyword) len=6 "throws"
// interfaces_iter_core.baml:177:63 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:178:5 (class) len=6 "Repeat"
// interfaces_iter_core.baml:178:11 (operator) len=1 "<"
// interfaces_iter_core.baml:178:12 (type) len=1 "T"
// interfaces_iter_core.baml:178:13 (operator) len=1 ">"
// interfaces_iter_core.baml:178:17 (property) len=5 "value"
// interfaces_iter_core.baml:178:24 (parameter) len=5 "value"
// interfaces_iter_core.baml:178:31 (property) len=5 "count"
// interfaces_iter_core.baml:178:38 (parameter) len=5 "count"
// interfaces_iter_core.baml:181:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:181:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:181:22 (operator) len=1 "<"
// interfaces_iter_core.baml:181:23 (type) len=1 "T"
// interfaces_iter_core.baml:181:26 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:181:31 (operator) len=1 ">"
// interfaces_iter_core.baml:182:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:182:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:182:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:182:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:182:36 (operator) len=1 "<"
// interfaces_iter_core.baml:182:37 (type) len=1 "T"
// interfaces_iter_core.baml:182:40 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:182:45 (operator) len=1 ">"
// interfaces_iter_core.baml:182:47 (keyword) len=6 "throws"
// interfaces_iter_core.baml:182:54 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:182:62 (parameter) len=4 "self"
// interfaces_iter_core.baml:185:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:185:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:185:22 (operator) len=1 "<"
// interfaces_iter_core.baml:185:23 (type) len=1 "T"
// interfaces_iter_core.baml:185:26 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:185:31 (operator) len=1 ">"
// interfaces_iter_core.baml:186:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:186:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:186:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:186:28 (type) len=1 "T"
// interfaces_iter_core.baml:186:30 (operator) len=1 "|"
// interfaces_iter_core.baml:186:32 (class) len=4 "Done"
// interfaces_iter_core.baml:186:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:186:44 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:187:7 (keyword) len=2 "if"
// interfaces_iter_core.baml:187:11 (parameter) len=4 "self"
// interfaces_iter_core.baml:187:16 (property) len=5 "count"
// interfaces_iter_core.baml:187:22 (operator) len=1 "<"
// interfaces_iter_core.baml:187:24 (number) len=1 "0"
// interfaces_iter_core.baml:188:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:188:14 (property) len=5 "value"
// interfaces_iter_core.baml:189:9 (keyword) len=4 "else"
// interfaces_iter_core.baml:189:14 (keyword) len=2 "if"
// interfaces_iter_core.baml:189:18 (parameter) len=4 "self"
// interfaces_iter_core.baml:189:23 (property) len=5 "count"
// interfaces_iter_core.baml:189:29 (operator) len=1 ">"
// interfaces_iter_core.baml:189:31 (number) len=1 "0"
// interfaces_iter_core.baml:190:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:190:14 (property) len=5 "count"
// interfaces_iter_core.baml:190:20 (operator) len=2 "-="
// interfaces_iter_core.baml:190:23 (number) len=1 "1"
// interfaces_iter_core.baml:191:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:191:14 (property) len=5 "value"
// interfaces_iter_core.baml:192:9 (keyword) len=4 "else"
// interfaces_iter_core.baml:193:9 (class) len=4 "Done"
// interfaces_iter_core.baml:199:1 (keyword) len=9 "interface"
// interfaces_iter_core.baml:199:11 (interface) [declaration] len=14 "OutOfBodyValue"
// interfaces_iter_core.baml:199:25 (operator) len=1 "<"
// interfaces_iter_core.baml:199:26 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:199:27 (operator) len=1 ">"
// interfaces_iter_core.baml:200:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:200:12 (method) [declaration] len=3 "get"
// interfaces_iter_core.baml:200:16 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:200:25 (type) len=1 "T"
// interfaces_iter_core.baml:200:27 (keyword) len=6 "throws"
// interfaces_iter_core.baml:200:34 (type) [defaultLibrary] len=7 "unknown"
// interfaces_iter_core.baml:203:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:203:7 (class) [declaration] len=12 "OutOfBodyBox"
// interfaces_iter_core.baml:203:19 (operator) len=1 "<"
// interfaces_iter_core.baml:203:20 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:203:21 (operator) len=1 ">"
// interfaces_iter_core.baml:204:3 (property) [declaration] len=5 "value"
// interfaces_iter_core.baml:204:10 (type) len=1 "T"
// interfaces_iter_core.baml:206:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:206:12 (method) [declaration] len=3 "new"
// interfaces_iter_core.baml:206:16 (parameter) [declaration] len=5 "value"
// interfaces_iter_core.baml:206:23 (type) len=1 "T"
// interfaces_iter_core.baml:206:29 (class) len=12 "OutOfBodyBox"
// interfaces_iter_core.baml:206:41 (operator) len=1 "<"
// interfaces_iter_core.baml:206:42 (type) len=1 "T"
// interfaces_iter_core.baml:206:43 (operator) len=1 ">"
// interfaces_iter_core.baml:206:45 (keyword) len=6 "throws"
// interfaces_iter_core.baml:206:52 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:207:5 (class) len=12 "OutOfBodyBox"
// interfaces_iter_core.baml:207:17 (operator) len=1 "<"
// interfaces_iter_core.baml:207:18 (type) len=1 "T"
// interfaces_iter_core.baml:207:19 (operator) len=1 ">"
// interfaces_iter_core.baml:207:23 (property) len=5 "value"
// interfaces_iter_core.baml:207:30 (parameter) len=5 "value"
// interfaces_iter_core.baml:211:1 (keyword) len=10 "implements"
// interfaces_iter_core.baml:211:11 (operator) len=1 "<"
// interfaces_iter_core.baml:211:12 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:211:13 (operator) len=1 ">"
// interfaces_iter_core.baml:211:15 (interface) len=14 "OutOfBodyValue"
// interfaces_iter_core.baml:211:29 (operator) len=1 "<"
// interfaces_iter_core.baml:211:30 (type) len=1 "T"
// interfaces_iter_core.baml:211:31 (operator) len=1 ">"
// interfaces_iter_core.baml:211:33 (keyword) len=3 "for"
// interfaces_iter_core.baml:211:37 (class) len=12 "OutOfBodyBox"
// interfaces_iter_core.baml:211:49 (operator) len=1 "<"
// interfaces_iter_core.baml:211:50 (type) len=1 "T"
// interfaces_iter_core.baml:211:51 (operator) len=1 ">"
// interfaces_iter_core.baml:212:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:212:12 (method) [declaration] len=3 "get"
// interfaces_iter_core.baml:212:16 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:212:25 (type) len=1 "T"
// interfaces_iter_core.baml:212:27 (keyword) len=6 "throws"
// interfaces_iter_core.baml:212:34 (type) [defaultLibrary] len=7 "unknown"
// interfaces_iter_core.baml:213:5 (parameter) len=4 "self"
// interfaces_iter_core.baml:213:10 (property) len=5 "value"
// interfaces_iter_core.baml:217:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:217:7 (class) [declaration] len=3 "Map"
// interfaces_iter_core.baml:217:10 (operator) len=1 "<"
// interfaces_iter_core.baml:217:11 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:217:14 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:217:17 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:217:20 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:217:22 (operator) len=1 ">"
// interfaces_iter_core.baml:218:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:218:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:218:17 (operator) len=1 "<"
// interfaces_iter_core.baml:218:18 (type) len=1 "T"
// interfaces_iter_core.baml:218:21 (type) len=1 "E"
// interfaces_iter_core.baml:218:22 (operator) len=1 ">"
// interfaces_iter_core.baml:219:3 (property) [declaration] len=4 "func"
// interfaces_iter_core.baml:219:10 (type) len=1 "T"
// interfaces_iter_core.baml:219:16 (type) len=1 "R"
// interfaces_iter_core.baml:219:18 (keyword) len=6 "throws"
// interfaces_iter_core.baml:219:25 (type) len=2 "E2"
// interfaces_iter_core.baml:221:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:221:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:221:22 (operator) len=1 "<"
// interfaces_iter_core.baml:221:23 (type) len=1 "R"
// interfaces_iter_core.baml:221:26 (type) len=1 "E"
// interfaces_iter_core.baml:221:28 (operator) len=1 "|"
// interfaces_iter_core.baml:221:30 (type) len=2 "E2"
// interfaces_iter_core.baml:221:32 (operator) len=1 ">"
// interfaces_iter_core.baml:222:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:222:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:222:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:222:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:222:36 (operator) len=1 "<"
// interfaces_iter_core.baml:222:37 (type) len=1 "R"
// interfaces_iter_core.baml:222:40 (type) len=1 "E"
// interfaces_iter_core.baml:222:42 (operator) len=1 "|"
// interfaces_iter_core.baml:222:44 (type) len=2 "E2"
// interfaces_iter_core.baml:222:46 (operator) len=1 ">"
// interfaces_iter_core.baml:222:48 (keyword) len=6 "throws"
// interfaces_iter_core.baml:222:55 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:222:63 (parameter) len=4 "self"
// interfaces_iter_core.baml:225:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:225:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:225:22 (operator) len=1 "<"
// interfaces_iter_core.baml:225:23 (type) len=1 "R"
// interfaces_iter_core.baml:225:26 (type) len=1 "E"
// interfaces_iter_core.baml:225:28 (operator) len=1 "|"
// interfaces_iter_core.baml:225:30 (type) len=2 "E2"
// interfaces_iter_core.baml:225:32 (operator) len=1 ">"
// interfaces_iter_core.baml:226:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:226:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:226:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:226:28 (type) len=1 "R"
// interfaces_iter_core.baml:226:30 (operator) len=1 "|"
// interfaces_iter_core.baml:226:32 (class) len=4 "Done"
// interfaces_iter_core.baml:226:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:226:44 (type) len=1 "E"
// interfaces_iter_core.baml:226:46 (operator) len=1 "|"
// interfaces_iter_core.baml:226:48 (type) len=2 "E2"
// interfaces_iter_core.baml:227:7 (keyword) len=5 "match"
// interfaces_iter_core.baml:227:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:227:19 (property) len=4 "iter"
// interfaces_iter_core.baml:227:24 (method) len=4 "next"
// interfaces_iter_core.baml:228:9 (class) len=4 "Done"
// interfaces_iter_core.baml:228:17 (class) len=4 "Done"
// interfaces_iter_core.baml:229:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:229:13 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:229:16 (type) len=1 "T"
// interfaces_iter_core.baml:229:21 (parameter) len=4 "self"
// interfaces_iter_core.baml:229:26 (property) len=4 "func"
// interfaces_iter_core.baml:229:31 (variable) len=1 "x"
// interfaces_iter_core.baml:235:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:235:7 (class) [declaration] len=6 "Filter"
// interfaces_iter_core.baml:235:13 (operator) len=1 "<"
// interfaces_iter_core.baml:235:14 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:235:17 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:235:20 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:235:22 (operator) len=1 ">"
// interfaces_iter_core.baml:236:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:236:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:236:17 (operator) len=1 "<"
// interfaces_iter_core.baml:236:18 (type) len=1 "T"
// interfaces_iter_core.baml:236:21 (type) len=1 "E"
// interfaces_iter_core.baml:236:22 (operator) len=1 ">"
// interfaces_iter_core.baml:237:3 (property) [declaration] len=9 "predicate"
// interfaces_iter_core.baml:237:15 (type) len=1 "T"
// interfaces_iter_core.baml:237:21 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:237:26 (keyword) len=6 "throws"
// interfaces_iter_core.baml:237:33 (type) len=2 "E2"
// interfaces_iter_core.baml:239:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:239:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:239:22 (operator) len=1 "<"
// interfaces_iter_core.baml:239:23 (type) len=1 "T"
// interfaces_iter_core.baml:239:26 (type) len=1 "E"
// interfaces_iter_core.baml:239:28 (operator) len=1 "|"
// interfaces_iter_core.baml:239:30 (type) len=2 "E2"
// interfaces_iter_core.baml:239:32 (operator) len=1 ">"
// interfaces_iter_core.baml:240:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:240:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:240:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:240:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:240:36 (operator) len=1 "<"
// interfaces_iter_core.baml:240:37 (type) len=1 "T"
// interfaces_iter_core.baml:240:40 (type) len=1 "E"
// interfaces_iter_core.baml:240:42 (operator) len=1 "|"
// interfaces_iter_core.baml:240:44 (type) len=2 "E2"
// interfaces_iter_core.baml:240:46 (operator) len=1 ">"
// interfaces_iter_core.baml:240:48 (keyword) len=6 "throws"
// interfaces_iter_core.baml:240:55 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:240:63 (parameter) len=4 "self"
// interfaces_iter_core.baml:243:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:243:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:243:22 (operator) len=1 "<"
// interfaces_iter_core.baml:243:23 (type) len=1 "T"
// interfaces_iter_core.baml:243:26 (type) len=1 "E"
// interfaces_iter_core.baml:243:28 (operator) len=1 "|"
// interfaces_iter_core.baml:243:30 (type) len=2 "E2"
// interfaces_iter_core.baml:243:32 (operator) len=1 ">"
// interfaces_iter_core.baml:244:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:244:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:244:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:244:28 (type) len=1 "T"
// interfaces_iter_core.baml:244:30 (operator) len=1 "|"
// interfaces_iter_core.baml:244:32 (class) len=4 "Done"
// interfaces_iter_core.baml:244:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:244:44 (type) len=1 "E"
// interfaces_iter_core.baml:244:46 (operator) len=1 "|"
// interfaces_iter_core.baml:244:48 (type) len=2 "E2"
// interfaces_iter_core.baml:245:7 (keyword) len=5 "while"
// interfaces_iter_core.baml:245:14 (boolean) len=4 "true"
// interfaces_iter_core.baml:246:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:246:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:246:21 (property) len=4 "iter"
// interfaces_iter_core.baml:246:26 (method) len=4 "next"
// interfaces_iter_core.baml:247:11 (class) len=4 "Done"
// interfaces_iter_core.baml:247:21 (keyword) len=6 "return"
// interfaces_iter_core.baml:247:28 (class) len=4 "Done"
// interfaces_iter_core.baml:248:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:248:15 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:248:18 (type) len=1 "T"
// interfaces_iter_core.baml:249:13 (keyword) len=2 "if"
// interfaces_iter_core.baml:249:17 (parameter) len=4 "self"
// interfaces_iter_core.baml:249:22 (property) len=9 "predicate"
// interfaces_iter_core.baml:249:32 (variable) len=1 "x"
// interfaces_iter_core.baml:250:15 (keyword) len=6 "return"
// interfaces_iter_core.baml:250:22 (variable) len=1 "x"
// interfaces_iter_core.baml:255:7 (class) len=4 "Done"
// interfaces_iter_core.baml:260:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:260:7 (class) [declaration] len=9 "FilterMap"
// interfaces_iter_core.baml:260:16 (operator) len=1 "<"
// interfaces_iter_core.baml:260:17 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:260:20 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:260:23 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:260:26 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:260:28 (operator) len=1 ">"
// interfaces_iter_core.baml:261:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:261:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:261:17 (operator) len=1 "<"
// interfaces_iter_core.baml:261:18 (type) len=1 "T"
// interfaces_iter_core.baml:261:21 (type) len=1 "E"
// interfaces_iter_core.baml:261:22 (operator) len=1 ">"
// interfaces_iter_core.baml:262:3 (property) [declaration] len=4 "func"
// interfaces_iter_core.baml:262:10 (type) len=1 "T"
// interfaces_iter_core.baml:262:16 (type) len=1 "R"
// interfaces_iter_core.baml:262:19 (keyword) len=6 "throws"
// interfaces_iter_core.baml:262:26 (type) len=2 "E2"
// interfaces_iter_core.baml:264:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:264:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:264:22 (operator) len=1 "<"
// interfaces_iter_core.baml:264:23 (type) len=1 "R"
// interfaces_iter_core.baml:264:26 (type) len=1 "E"
// interfaces_iter_core.baml:264:28 (operator) len=1 "|"
// interfaces_iter_core.baml:264:30 (type) len=2 "E2"
// interfaces_iter_core.baml:264:32 (operator) len=1 ">"
// interfaces_iter_core.baml:265:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:265:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:265:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:265:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:265:36 (operator) len=1 "<"
// interfaces_iter_core.baml:265:37 (type) len=1 "R"
// interfaces_iter_core.baml:265:40 (type) len=1 "E"
// interfaces_iter_core.baml:265:42 (operator) len=1 "|"
// interfaces_iter_core.baml:265:44 (type) len=2 "E2"
// interfaces_iter_core.baml:265:46 (operator) len=1 ">"
// interfaces_iter_core.baml:265:48 (keyword) len=6 "throws"
// interfaces_iter_core.baml:265:55 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:265:63 (parameter) len=4 "self"
// interfaces_iter_core.baml:268:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:268:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:268:22 (operator) len=1 "<"
// interfaces_iter_core.baml:268:23 (type) len=1 "R"
// interfaces_iter_core.baml:268:26 (type) len=1 "E"
// interfaces_iter_core.baml:268:28 (operator) len=1 "|"
// interfaces_iter_core.baml:268:30 (type) len=2 "E2"
// interfaces_iter_core.baml:268:32 (operator) len=1 ">"
// interfaces_iter_core.baml:269:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:269:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:269:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:269:28 (type) len=1 "R"
// interfaces_iter_core.baml:269:30 (operator) len=1 "|"
// interfaces_iter_core.baml:269:32 (class) len=4 "Done"
// interfaces_iter_core.baml:269:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:269:44 (type) len=1 "E"
// interfaces_iter_core.baml:269:46 (operator) len=1 "|"
// interfaces_iter_core.baml:269:48 (type) len=2 "E2"
// interfaces_iter_core.baml:270:7 (keyword) len=5 "while"
// interfaces_iter_core.baml:270:14 (boolean) len=4 "true"
// interfaces_iter_core.baml:271:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:271:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:271:21 (property) len=4 "iter"
// interfaces_iter_core.baml:271:26 (method) len=4 "next"
// interfaces_iter_core.baml:272:11 (class) len=4 "Done"
// interfaces_iter_core.baml:272:21 (keyword) len=6 "return"
// interfaces_iter_core.baml:272:28 (class) len=4 "Done"
// interfaces_iter_core.baml:273:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:273:15 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:273:18 (type) len=1 "T"
// interfaces_iter_core.baml:274:13 (keyword) len=5 "match"
// interfaces_iter_core.baml:274:20 (parameter) len=4 "self"
// interfaces_iter_core.baml:274:25 (property) len=4 "func"
// interfaces_iter_core.baml:274:30 (variable) len=1 "x"
// interfaces_iter_core.baml:275:15 (type) [defaultLibrary] len=4 "null"
// interfaces_iter_core.baml:276:15 (keyword) len=3 "let"
// interfaces_iter_core.baml:276:19 (variable) [declaration] len=1 "y"
// interfaces_iter_core.baml:276:22 (type) len=1 "R"
// interfaces_iter_core.baml:276:29 (keyword) len=6 "return"
// interfaces_iter_core.baml:276:36 (variable) len=1 "y"
// interfaces_iter_core.baml:281:7 (class) len=4 "Done"
// interfaces_iter_core.baml:286:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:286:7 (class) [declaration] len=7 "FlatMap"
// interfaces_iter_core.baml:286:14 (operator) len=1 "<"
// interfaces_iter_core.baml:286:15 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:286:18 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:286:21 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:286:24 (typeParameter) [declaration] len=2 "E2"
// interfaces_iter_core.baml:286:28 (typeParameter) [declaration] len=2 "E3"
// interfaces_iter_core.baml:286:30 (operator) len=1 ">"
// interfaces_iter_core.baml:287:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:287:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:287:17 (operator) len=1 "<"
// interfaces_iter_core.baml:287:18 (type) len=1 "T"
// interfaces_iter_core.baml:287:21 (type) len=1 "E"
// interfaces_iter_core.baml:287:22 (operator) len=1 ">"
// interfaces_iter_core.baml:288:3 (property) [declaration] len=4 "func"
// interfaces_iter_core.baml:288:10 (type) len=1 "T"
// interfaces_iter_core.baml:288:16 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:288:24 (operator) len=1 "<"
// interfaces_iter_core.baml:288:25 (type) len=1 "R"
// interfaces_iter_core.baml:288:28 (type) len=2 "E3"
// interfaces_iter_core.baml:288:30 (operator) len=1 ">"
// interfaces_iter_core.baml:288:32 (keyword) len=6 "throws"
// interfaces_iter_core.baml:288:39 (type) len=2 "E2"
// interfaces_iter_core.baml:289:3 (property) [declaration] len=5 "inner"
// interfaces_iter_core.baml:289:10 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:289:18 (operator) len=1 "<"
// interfaces_iter_core.baml:289:19 (type) len=1 "R"
// interfaces_iter_core.baml:289:22 (type) len=2 "E3"
// interfaces_iter_core.baml:289:24 (operator) len=1 ">"
// interfaces_iter_core.baml:289:26 (operator) len=1 "|"
// interfaces_iter_core.baml:289:28 (class) len=4 "Done"
// interfaces_iter_core.baml:291:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:291:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:291:22 (operator) len=1 "<"
// interfaces_iter_core.baml:291:23 (type) len=1 "R"
// interfaces_iter_core.baml:291:26 (type) len=1 "E"
// interfaces_iter_core.baml:291:28 (operator) len=1 "|"
// interfaces_iter_core.baml:291:30 (type) len=2 "E2"
// interfaces_iter_core.baml:291:33 (operator) len=1 "|"
// interfaces_iter_core.baml:291:35 (type) len=2 "E3"
// interfaces_iter_core.baml:291:37 (operator) len=1 ">"
// interfaces_iter_core.baml:292:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:292:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:292:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:292:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:292:36 (operator) len=1 "<"
// interfaces_iter_core.baml:292:37 (type) len=1 "R"
// interfaces_iter_core.baml:292:40 (type) len=1 "E"
// interfaces_iter_core.baml:292:42 (operator) len=1 "|"
// interfaces_iter_core.baml:292:44 (type) len=2 "E2"
// interfaces_iter_core.baml:292:47 (operator) len=1 "|"
// interfaces_iter_core.baml:292:49 (type) len=2 "E3"
// interfaces_iter_core.baml:292:51 (operator) len=1 ">"
// interfaces_iter_core.baml:292:53 (keyword) len=6 "throws"
// interfaces_iter_core.baml:292:60 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:292:68 (parameter) len=4 "self"
// interfaces_iter_core.baml:295:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:295:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:295:22 (operator) len=1 "<"
// interfaces_iter_core.baml:295:23 (type) len=1 "R"
// interfaces_iter_core.baml:295:26 (type) len=1 "E"
// interfaces_iter_core.baml:295:28 (operator) len=1 "|"
// interfaces_iter_core.baml:295:30 (type) len=2 "E2"
// interfaces_iter_core.baml:295:33 (operator) len=1 "|"
// interfaces_iter_core.baml:295:35 (type) len=2 "E3"
// interfaces_iter_core.baml:295:37 (operator) len=1 ">"
// interfaces_iter_core.baml:296:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:296:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:296:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:296:28 (type) len=1 "R"
// interfaces_iter_core.baml:296:30 (operator) len=1 "|"
// interfaces_iter_core.baml:296:32 (class) len=4 "Done"
// interfaces_iter_core.baml:296:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:296:44 (type) len=1 "E"
// interfaces_iter_core.baml:296:46 (operator) len=1 "|"
// interfaces_iter_core.baml:296:48 (type) len=2 "E2"
// interfaces_iter_core.baml:296:51 (operator) len=1 "|"
// interfaces_iter_core.baml:296:53 (type) len=2 "E3"
// interfaces_iter_core.baml:297:7 (keyword) len=5 "while"
// interfaces_iter_core.baml:297:14 (boolean) len=4 "true"
// interfaces_iter_core.baml:298:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:298:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:298:21 (property) len=5 "inner"
// interfaces_iter_core.baml:299:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:299:15 (variable) [declaration] len=3 "cur"
// interfaces_iter_core.baml:299:20 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:299:28 (operator) len=1 "<"
// interfaces_iter_core.baml:299:29 (type) len=1 "R"
// interfaces_iter_core.baml:299:32 (type) len=2 "E3"
// interfaces_iter_core.baml:299:34 (operator) len=1 ">"
// interfaces_iter_core.baml:300:13 (keyword) len=5 "match"
// interfaces_iter_core.baml:300:20 (variable) len=3 "cur"
// interfaces_iter_core.baml:300:24 (method) len=4 "next"
// interfaces_iter_core.baml:301:15 (class) len=4 "Done"
// interfaces_iter_core.baml:301:25 (parameter) len=4 "self"
// interfaces_iter_core.baml:301:30 (property) len=5 "inner"
// interfaces_iter_core.baml:301:36 (operator) len=1 "="
// interfaces_iter_core.baml:301:38 (class) len=4 "Done"
// interfaces_iter_core.baml:302:15 (keyword) len=3 "let"
// interfaces_iter_core.baml:302:19 (variable) [declaration] len=1 "y"
// interfaces_iter_core.baml:302:22 (type) len=1 "R"
// interfaces_iter_core.baml:302:29 (keyword) len=6 "return"
// interfaces_iter_core.baml:302:36 (variable) len=1 "y"
// interfaces_iter_core.baml:305:11 (class) len=4 "Done"
// interfaces_iter_core.baml:306:13 (keyword) len=5 "match"
// interfaces_iter_core.baml:306:20 (parameter) len=4 "self"
// interfaces_iter_core.baml:306:25 (property) len=4 "iter"
// interfaces_iter_core.baml:306:30 (method) len=4 "next"
// interfaces_iter_core.baml:307:15 (class) len=4 "Done"
// interfaces_iter_core.baml:307:25 (keyword) len=6 "return"
// interfaces_iter_core.baml:307:32 (class) len=4 "Done"
// interfaces_iter_core.baml:308:15 (keyword) len=3 "let"
// interfaces_iter_core.baml:308:19 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:308:22 (type) len=1 "T"
// interfaces_iter_core.baml:309:17 (keyword) len=3 "let"
// interfaces_iter_core.baml:309:21 (variable) [declaration] len=2 "it"
// interfaces_iter_core.baml:309:24 (operator) len=1 "="
// interfaces_iter_core.baml:309:26 (parameter) len=4 "self"
// interfaces_iter_core.baml:309:31 (property) len=4 "func"
// interfaces_iter_core.baml:309:36 (variable) len=1 "x"
// interfaces_iter_core.baml:309:39 (method) len=4 "iter"
// interfaces_iter_core.baml:310:17 (parameter) len=4 "self"
// interfaces_iter_core.baml:310:22 (property) len=5 "inner"
// interfaces_iter_core.baml:310:28 (operator) len=1 "="
// interfaces_iter_core.baml:310:30 (variable) len=2 "it"
// interfaces_iter_core.baml:316:7 (class) len=4 "Done"
// interfaces_iter_core.baml:321:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:321:7 (class) [declaration] len=7 "Flatten"
// interfaces_iter_core.baml:321:14 (operator) len=1 "<"
// interfaces_iter_core.baml:321:15 (typeParameter) [declaration] len=1 "R"
// interfaces_iter_core.baml:321:18 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:321:21 (typeParameter) [declaration] len=2 "E3"
// interfaces_iter_core.baml:321:23 (operator) len=1 ">"
// interfaces_iter_core.baml:322:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:322:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:322:17 (operator) len=1 "<"
// interfaces_iter_core.baml:322:18 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:322:26 (operator) len=1 "<"
// interfaces_iter_core.baml:322:27 (type) len=1 "R"
// interfaces_iter_core.baml:322:30 (type) len=2 "E3"
// interfaces_iter_core.baml:322:32 (operator) len=1 ">"
// interfaces_iter_core.baml:322:35 (type) len=1 "E"
// interfaces_iter_core.baml:322:36 (operator) len=1 ">"
// interfaces_iter_core.baml:323:3 (property) [declaration] len=5 "inner"
// interfaces_iter_core.baml:323:10 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:323:18 (operator) len=1 "<"
// interfaces_iter_core.baml:323:19 (type) len=1 "R"
// interfaces_iter_core.baml:323:22 (type) len=2 "E3"
// interfaces_iter_core.baml:323:24 (operator) len=1 ">"
// interfaces_iter_core.baml:323:26 (operator) len=1 "|"
// interfaces_iter_core.baml:323:28 (class) len=4 "Done"
// interfaces_iter_core.baml:325:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:325:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:325:22 (operator) len=1 "<"
// interfaces_iter_core.baml:325:23 (type) len=1 "R"
// interfaces_iter_core.baml:325:26 (type) len=1 "E"
// interfaces_iter_core.baml:325:28 (operator) len=1 "|"
// interfaces_iter_core.baml:325:30 (type) len=2 "E3"
// interfaces_iter_core.baml:325:32 (operator) len=1 ">"
// interfaces_iter_core.baml:326:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:326:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:326:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:326:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:326:36 (operator) len=1 "<"
// interfaces_iter_core.baml:326:37 (type) len=1 "R"
// interfaces_iter_core.baml:326:40 (type) len=1 "E"
// interfaces_iter_core.baml:326:42 (operator) len=1 "|"
// interfaces_iter_core.baml:326:44 (type) len=2 "E3"
// interfaces_iter_core.baml:326:46 (operator) len=1 ">"
// interfaces_iter_core.baml:326:48 (keyword) len=6 "throws"
// interfaces_iter_core.baml:326:55 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:326:63 (parameter) len=4 "self"
// interfaces_iter_core.baml:329:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:329:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:329:22 (operator) len=1 "<"
// interfaces_iter_core.baml:329:23 (type) len=1 "R"
// interfaces_iter_core.baml:329:26 (type) len=1 "E"
// interfaces_iter_core.baml:329:28 (operator) len=1 "|"
// interfaces_iter_core.baml:329:30 (type) len=2 "E3"
// interfaces_iter_core.baml:329:32 (operator) len=1 ">"
// interfaces_iter_core.baml:330:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:330:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:330:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:330:28 (type) len=1 "R"
// interfaces_iter_core.baml:330:30 (operator) len=1 "|"
// interfaces_iter_core.baml:330:32 (class) len=4 "Done"
// interfaces_iter_core.baml:330:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:330:44 (type) len=1 "E"
// interfaces_iter_core.baml:330:46 (operator) len=1 "|"
// interfaces_iter_core.baml:330:48 (type) len=2 "E3"
// interfaces_iter_core.baml:331:7 (keyword) len=5 "while"
// interfaces_iter_core.baml:331:14 (boolean) len=4 "true"
// interfaces_iter_core.baml:332:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:332:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:332:21 (property) len=5 "inner"
// interfaces_iter_core.baml:333:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:333:15 (variable) [declaration] len=3 "cur"
// interfaces_iter_core.baml:333:20 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:333:28 (operator) len=1 "<"
// interfaces_iter_core.baml:333:29 (type) len=1 "R"
// interfaces_iter_core.baml:333:32 (type) len=2 "E3"
// interfaces_iter_core.baml:333:34 (operator) len=1 ">"
// interfaces_iter_core.baml:334:13 (keyword) len=5 "match"
// interfaces_iter_core.baml:334:20 (variable) len=3 "cur"
// interfaces_iter_core.baml:334:24 (method) len=4 "next"
// interfaces_iter_core.baml:335:15 (class) len=4 "Done"
// interfaces_iter_core.baml:335:25 (parameter) len=4 "self"
// interfaces_iter_core.baml:335:30 (property) len=5 "inner"
// interfaces_iter_core.baml:335:36 (operator) len=1 "="
// interfaces_iter_core.baml:335:38 (class) len=4 "Done"
// interfaces_iter_core.baml:336:15 (keyword) len=3 "let"
// interfaces_iter_core.baml:336:19 (variable) [declaration] len=1 "y"
// interfaces_iter_core.baml:336:22 (type) len=1 "R"
// interfaces_iter_core.baml:336:29 (keyword) len=6 "return"
// interfaces_iter_core.baml:336:36 (variable) len=1 "y"
// interfaces_iter_core.baml:339:11 (class) len=4 "Done"
// interfaces_iter_core.baml:340:13 (keyword) len=5 "match"
// interfaces_iter_core.baml:340:20 (parameter) len=4 "self"
// interfaces_iter_core.baml:340:25 (property) len=4 "iter"
// interfaces_iter_core.baml:340:30 (method) len=4 "next"
// interfaces_iter_core.baml:341:15 (class) len=4 "Done"
// interfaces_iter_core.baml:341:25 (keyword) len=6 "return"
// interfaces_iter_core.baml:341:32 (class) len=4 "Done"
// interfaces_iter_core.baml:342:15 (keyword) len=3 "let"
// interfaces_iter_core.baml:342:19 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:342:22 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:342:30 (operator) len=1 "<"
// interfaces_iter_core.baml:342:31 (type) len=1 "R"
// interfaces_iter_core.baml:342:34 (type) len=2 "E3"
// interfaces_iter_core.baml:342:36 (operator) len=1 ">"
// interfaces_iter_core.baml:343:17 (keyword) len=3 "let"
// interfaces_iter_core.baml:343:21 (variable) [declaration] len=2 "it"
// interfaces_iter_core.baml:343:24 (operator) len=1 "="
// interfaces_iter_core.baml:343:26 (variable) len=1 "x"
// interfaces_iter_core.baml:343:28 (method) len=4 "iter"
// interfaces_iter_core.baml:344:17 (parameter) len=4 "self"
// interfaces_iter_core.baml:344:22 (property) len=5 "inner"
// interfaces_iter_core.baml:344:28 (operator) len=1 "="
// interfaces_iter_core.baml:344:30 (variable) len=2 "it"
// interfaces_iter_core.baml:350:7 (class) len=4 "Done"
// interfaces_iter_core.baml:355:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:355:7 (class) [declaration] len=8 "Peekable"
// interfaces_iter_core.baml:355:15 (operator) len=1 "<"
// interfaces_iter_core.baml:355:16 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:355:19 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:355:20 (operator) len=1 ">"
// interfaces_iter_core.baml:356:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:356:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:356:17 (operator) len=1 "<"
// interfaces_iter_core.baml:356:18 (type) len=1 "T"
// interfaces_iter_core.baml:356:21 (type) len=1 "E"
// interfaces_iter_core.baml:356:22 (operator) len=1 ">"
// interfaces_iter_core.baml:357:3 (property) [declaration] len=6 "buffer"
// interfaces_iter_core.baml:357:11 (type) len=1 "T"
// interfaces_iter_core.baml:357:13 (operator) len=1 "|"
// interfaces_iter_core.baml:357:15 (class) len=4 "Done"
// interfaces_iter_core.baml:358:3 (property) [declaration] len=10 "has_buffer"
// interfaces_iter_core.baml:358:15 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:360:3 (keyword) len=8 "function"
// interfaces_iter_core.baml:360:12 (method) [declaration] len=4 "peek"
// interfaces_iter_core.baml:360:17 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:360:26 (type) len=1 "T"
// interfaces_iter_core.baml:360:28 (operator) len=1 "|"
// interfaces_iter_core.baml:360:30 (class) len=4 "Done"
// interfaces_iter_core.baml:360:35 (keyword) len=6 "throws"
// interfaces_iter_core.baml:360:42 (type) len=1 "E"
// interfaces_iter_core.baml:361:5 (keyword) len=2 "if"
// interfaces_iter_core.baml:361:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:361:14 (property) len=10 "has_buffer"
// interfaces_iter_core.baml:362:7 (keyword) len=6 "return"
// interfaces_iter_core.baml:362:14 (parameter) len=4 "self"
// interfaces_iter_core.baml:362:19 (property) len=6 "buffer"
// interfaces_iter_core.baml:364:5 (keyword) len=3 "let"
// interfaces_iter_core.baml:364:9 (variable) [declaration] len=1 "v"
// interfaces_iter_core.baml:364:11 (operator) len=1 "="
// interfaces_iter_core.baml:364:13 (parameter) len=4 "self"
// interfaces_iter_core.baml:364:18 (property) len=4 "iter"
// interfaces_iter_core.baml:364:23 (method) len=4 "next"
// interfaces_iter_core.baml:365:5 (parameter) len=4 "self"
// interfaces_iter_core.baml:365:10 (property) len=6 "buffer"
// interfaces_iter_core.baml:365:17 (operator) len=1 "="
// interfaces_iter_core.baml:365:19 (variable) len=1 "v"
// interfaces_iter_core.baml:366:5 (parameter) len=4 "self"
// interfaces_iter_core.baml:366:10 (property) len=10 "has_buffer"
// interfaces_iter_core.baml:366:21 (operator) len=1 "="
// interfaces_iter_core.baml:366:23 (boolean) len=4 "true"
// interfaces_iter_core.baml:367:5 (variable) len=1 "v"
// interfaces_iter_core.baml:370:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:370:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:370:22 (operator) len=1 "<"
// interfaces_iter_core.baml:370:23 (type) len=1 "T"
// interfaces_iter_core.baml:370:26 (type) len=1 "E"
// interfaces_iter_core.baml:370:27 (operator) len=1 ">"
// interfaces_iter_core.baml:371:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:371:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:371:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:371:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:371:36 (operator) len=1 "<"
// interfaces_iter_core.baml:371:37 (type) len=1 "T"
// interfaces_iter_core.baml:371:40 (type) len=1 "E"
// interfaces_iter_core.baml:371:41 (operator) len=1 ">"
// interfaces_iter_core.baml:371:43 (keyword) len=6 "throws"
// interfaces_iter_core.baml:371:50 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:371:58 (parameter) len=4 "self"
// interfaces_iter_core.baml:374:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:374:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:374:22 (operator) len=1 "<"
// interfaces_iter_core.baml:374:23 (type) len=1 "T"
// interfaces_iter_core.baml:374:26 (type) len=1 "E"
// interfaces_iter_core.baml:374:27 (operator) len=1 ">"
// interfaces_iter_core.baml:375:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:375:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:375:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:375:28 (type) len=1 "T"
// interfaces_iter_core.baml:375:30 (operator) len=1 "|"
// interfaces_iter_core.baml:375:32 (class) len=4 "Done"
// interfaces_iter_core.baml:375:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:375:44 (type) len=1 "E"
// interfaces_iter_core.baml:376:7 (keyword) len=2 "if"
// interfaces_iter_core.baml:376:11 (parameter) len=4 "self"
// interfaces_iter_core.baml:376:16 (property) len=10 "has_buffer"
// interfaces_iter_core.baml:377:9 (keyword) len=3 "let"
// interfaces_iter_core.baml:377:13 (variable) [declaration] len=1 "v"
// interfaces_iter_core.baml:377:15 (operator) len=1 "="
// interfaces_iter_core.baml:377:17 (parameter) len=4 "self"
// interfaces_iter_core.baml:377:22 (property) len=6 "buffer"
// interfaces_iter_core.baml:378:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:378:14 (property) len=6 "buffer"
// interfaces_iter_core.baml:378:21 (operator) len=1 "="
// interfaces_iter_core.baml:378:23 (class) len=4 "Done"
// interfaces_iter_core.baml:379:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:379:14 (property) len=10 "has_buffer"
// interfaces_iter_core.baml:379:25 (operator) len=1 "="
// interfaces_iter_core.baml:379:27 (boolean) len=5 "false"
// interfaces_iter_core.baml:380:9 (keyword) len=6 "return"
// interfaces_iter_core.baml:380:16 (variable) len=1 "v"
// interfaces_iter_core.baml:382:7 (parameter) len=4 "self"
// interfaces_iter_core.baml:382:12 (property) len=4 "iter"
// interfaces_iter_core.baml:382:17 (method) len=4 "next"
// interfaces_iter_core.baml:387:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:387:7 (class) [declaration] len=6 "StepBy"
// interfaces_iter_core.baml:387:13 (operator) len=1 "<"
// interfaces_iter_core.baml:387:14 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:387:17 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:387:18 (operator) len=1 ">"
// interfaces_iter_core.baml:388:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:388:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:388:17 (operator) len=1 "<"
// interfaces_iter_core.baml:388:18 (type) len=1 "T"
// interfaces_iter_core.baml:388:21 (type) len=1 "E"
// interfaces_iter_core.baml:388:22 (operator) len=1 ">"
// interfaces_iter_core.baml:389:3 (property) [declaration] len=1 "n"
// interfaces_iter_core.baml:389:6 (type) [defaultLibrary] len=3 "int"
// interfaces_iter_core.baml:390:3 (property) [declaration] len=5 "first"
// interfaces_iter_core.baml:390:10 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:392:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:392:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:392:22 (operator) len=1 "<"
// interfaces_iter_core.baml:392:23 (type) len=1 "T"
// interfaces_iter_core.baml:392:26 (type) len=1 "E"
// interfaces_iter_core.baml:392:27 (operator) len=1 ">"
// interfaces_iter_core.baml:393:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:393:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:393:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:393:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:393:36 (operator) len=1 "<"
// interfaces_iter_core.baml:393:37 (type) len=1 "T"
// interfaces_iter_core.baml:393:40 (type) len=1 "E"
// interfaces_iter_core.baml:393:41 (operator) len=1 ">"
// interfaces_iter_core.baml:393:43 (keyword) len=6 "throws"
// interfaces_iter_core.baml:393:50 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:393:58 (parameter) len=4 "self"
// interfaces_iter_core.baml:396:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:396:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:396:22 (operator) len=1 "<"
// interfaces_iter_core.baml:396:23 (type) len=1 "T"
// interfaces_iter_core.baml:396:26 (type) len=1 "E"
// interfaces_iter_core.baml:396:27 (operator) len=1 ">"
// interfaces_iter_core.baml:397:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:397:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:397:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:397:28 (type) len=1 "T"
// interfaces_iter_core.baml:397:30 (operator) len=1 "|"
// interfaces_iter_core.baml:397:32 (class) len=4 "Done"
// interfaces_iter_core.baml:397:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:397:44 (type) len=1 "E"
// interfaces_iter_core.baml:398:7 (keyword) len=2 "if"
// interfaces_iter_core.baml:398:11 (parameter) len=4 "self"
// interfaces_iter_core.baml:398:16 (property) len=5 "first"
// interfaces_iter_core.baml:399:9 (parameter) len=4 "self"
// interfaces_iter_core.baml:399:14 (property) len=5 "first"
// interfaces_iter_core.baml:399:20 (operator) len=1 "="
// interfaces_iter_core.baml:399:22 (boolean) len=5 "false"
// interfaces_iter_core.baml:400:9 (keyword) len=6 "return"
// interfaces_iter_core.baml:400:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:400:21 (property) len=4 "iter"
// interfaces_iter_core.baml:400:26 (method) len=4 "next"
// interfaces_iter_core.baml:402:7 (keyword) len=3 "for"
// interfaces_iter_core.baml:402:12 (keyword) len=3 "let"
// interfaces_iter_core.baml:402:16 (variable) [declaration] len=1 "i"
// interfaces_iter_core.baml:402:18 (operator) len=1 "="
// interfaces_iter_core.baml:402:20 (number) len=1 "0"
// interfaces_iter_core.baml:402:23 (variable) len=1 "i"
// interfaces_iter_core.baml:402:25 (operator) len=1 "<"
// interfaces_iter_core.baml:402:27 (parameter) len=4 "self"
// interfaces_iter_core.baml:402:32 (property) len=1 "n"
// interfaces_iter_core.baml:402:34 (operator) len=1 "-"
// interfaces_iter_core.baml:402:36 (number) len=1 "1"
// interfaces_iter_core.baml:402:39 (variable) len=1 "i"
// interfaces_iter_core.baml:402:41 (operator) len=2 "+="
// interfaces_iter_core.baml:402:44 (number) len=1 "1"
// interfaces_iter_core.baml:403:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:403:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:403:21 (property) len=4 "iter"
// interfaces_iter_core.baml:403:26 (method) len=4 "next"
// interfaces_iter_core.baml:404:11 (class) len=4 "Done"
// interfaces_iter_core.baml:404:21 (keyword) len=6 "return"
// interfaces_iter_core.baml:404:28 (class) len=4 "Done"
// interfaces_iter_core.baml:405:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:405:15 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:405:18 (type) len=1 "T"
// interfaces_iter_core.baml:408:7 (parameter) len=4 "self"
// interfaces_iter_core.baml:408:12 (property) len=4 "iter"
// interfaces_iter_core.baml:408:17 (method) len=4 "next"
// interfaces_iter_core.baml:413:1 (keyword) len=5 "class"
// interfaces_iter_core.baml:413:7 (class) [declaration] len=5 "Chain"
// interfaces_iter_core.baml:413:12 (operator) len=1 "<"
// interfaces_iter_core.baml:413:13 (typeParameter) [declaration] len=1 "T"
// interfaces_iter_core.baml:413:16 (typeParameter) [declaration] len=1 "E"
// interfaces_iter_core.baml:413:17 (operator) len=1 ">"
// interfaces_iter_core.baml:414:3 (property) [declaration] len=4 "iter"
// interfaces_iter_core.baml:414:9 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:414:17 (operator) len=1 "<"
// interfaces_iter_core.baml:414:18 (type) len=1 "T"
// interfaces_iter_core.baml:414:21 (type) len=1 "E"
// interfaces_iter_core.baml:414:22 (operator) len=1 ">"
// interfaces_iter_core.baml:415:3 (property) [declaration] len=5 "other"
// interfaces_iter_core.baml:415:10 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:415:18 (operator) len=1 "<"
// interfaces_iter_core.baml:415:19 (type) len=1 "T"
// interfaces_iter_core.baml:415:22 (type) len=1 "E"
// interfaces_iter_core.baml:415:23 (operator) len=1 ">"
// interfaces_iter_core.baml:416:3 (property) [declaration] len=10 "first_done"
// interfaces_iter_core.baml:416:15 (type) [defaultLibrary] len=4 "bool"
// interfaces_iter_core.baml:418:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:418:14 (interface) len=8 "Iterable"
// interfaces_iter_core.baml:418:22 (operator) len=1 "<"
// interfaces_iter_core.baml:418:23 (type) len=1 "T"
// interfaces_iter_core.baml:418:26 (type) len=1 "E"
// interfaces_iter_core.baml:418:27 (operator) len=1 ">"
// interfaces_iter_core.baml:419:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:419:14 (method) [declaration] len=4 "iter"
// interfaces_iter_core.baml:419:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:419:28 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:419:36 (operator) len=1 "<"
// interfaces_iter_core.baml:419:37 (type) len=1 "T"
// interfaces_iter_core.baml:419:40 (type) len=1 "E"
// interfaces_iter_core.baml:419:41 (operator) len=1 ">"
// interfaces_iter_core.baml:419:43 (keyword) len=6 "throws"
// interfaces_iter_core.baml:419:50 (type) [defaultLibrary] len=5 "never"
// interfaces_iter_core.baml:419:58 (parameter) len=4 "self"
// interfaces_iter_core.baml:422:3 (keyword) len=10 "implements"
// interfaces_iter_core.baml:422:14 (interface) len=8 "Iterator"
// interfaces_iter_core.baml:422:22 (operator) len=1 "<"
// interfaces_iter_core.baml:422:23 (type) len=1 "T"
// interfaces_iter_core.baml:422:26 (type) len=1 "E"
// interfaces_iter_core.baml:422:27 (operator) len=1 ">"
// interfaces_iter_core.baml:423:5 (keyword) len=8 "function"
// interfaces_iter_core.baml:423:14 (method) [declaration] len=4 "next"
// interfaces_iter_core.baml:423:19 (parameter) [declaration] len=4 "self"
// interfaces_iter_core.baml:423:28 (type) len=1 "T"
// interfaces_iter_core.baml:423:30 (operator) len=1 "|"
// interfaces_iter_core.baml:423:32 (class) len=4 "Done"
// interfaces_iter_core.baml:423:37 (keyword) len=6 "throws"
// interfaces_iter_core.baml:423:44 (type) len=1 "E"
// interfaces_iter_core.baml:424:7 (keyword) len=2 "if"
// interfaces_iter_core.baml:424:11 (operator) len=1 "!"
// interfaces_iter_core.baml:424:12 (parameter) len=4 "self"
// interfaces_iter_core.baml:424:17 (property) len=10 "first_done"
// interfaces_iter_core.baml:425:9 (keyword) len=5 "match"
// interfaces_iter_core.baml:425:16 (parameter) len=4 "self"
// interfaces_iter_core.baml:425:21 (property) len=4 "iter"
// interfaces_iter_core.baml:425:26 (method) len=4 "next"
// interfaces_iter_core.baml:426:11 (class) len=4 "Done"
// interfaces_iter_core.baml:426:21 (parameter) len=4 "self"
// interfaces_iter_core.baml:426:26 (property) len=10 "first_done"
// interfaces_iter_core.baml:426:37 (operator) len=1 "="
// interfaces_iter_core.baml:426:39 (boolean) len=4 "true"
// interfaces_iter_core.baml:427:11 (keyword) len=3 "let"
// interfaces_iter_core.baml:427:15 (variable) [declaration] len=1 "x"
// interfaces_iter_core.baml:427:18 (type) len=1 "T"
// interfaces_iter_core.baml:427:25 (keyword) len=6 "return"
// interfaces_iter_core.baml:427:32 (variable) len=1 "x"
// interfaces_iter_core.baml:430:7 (parameter) len=4 "self"
// interfaces_iter_core.baml:430:12 (property) len=5 "other"
// interfaces_iter_core.baml:430:18 (method) len=4 "next"
