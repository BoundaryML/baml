/// Lazy adapter classes for the iterator protocol. Each adapter wraps an inner
/// iterator (plus a closure / count / other iterator as needed). The protocol
/// methods are attached via in-body `implements` blocks.
///
/// All adapters are constructed by the default adapter methods on `Iterator`
/// (see iter.baml) — constructing one does NO work; work happens on `next`.

/// `map`: apply `func` to each element. (`E` = source error, `E2` = fn error.)
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

/// `filter`: keep elements for which `predicate` is true.
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

/// `filter_map`: apply `func`, keep each non-null result.
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

/// `flat_map`: map each element to an `Iterable` and flatten the results.
/// `inner` holds the currently-draining sub-iterator (or `Done` when none).
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

/// `flatten`: flatten an iterator-of-iterables into one iterator. The source
/// iterator's element type is itself an `Iterable<R, E3>`.
/// `inner` holds the currently-draining sub-iterator (or `Done` when none).
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

/// `peekable`: supports one-element lookahead via `peek`.
/// `buffer` holds a peeked-but-not-yet-consumed element; `has_buffer` flags it.
class Peekable<T, E> {
  iter: Iterator<T, E>
  buffer: T | Done
  has_buffer: bool

  /// Return the next element WITHOUT consuming it, or `Done` if exhausted.
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

/// `step_by`: yield the first element, then every `n`-th element after.
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
      // Skip n-1 elements, then return the n-th.
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

/// `chain`: drain `self` fully, then drain `other`.
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
