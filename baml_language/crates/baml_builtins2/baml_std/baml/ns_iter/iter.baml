/// A sentinel class returned by `baml.iter.Iterator.next` when there are no more elements to yield.
class Done {
}

/// Implemented by types that can be iterated over.
interface Iterable {
  /// The type of the elements yielded by the iterator.
  type Item
  /// The type of error that can be thrown by the iterator during iteration (`never` if the iterator does not throw).
  type Error = never

  /// Creates an iterator over the value.
  function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
}

/// Implemented by types as a way to iterate over a sequence of values,
/// often from some container.
interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
/// The type of the elements yielded by the iterator.
  type Item
  /// The type of error that can be thrown by the iterator during iteration (`never` if the iterator does not throw).
  type Error = never

  /// Advances the iterator and returns the next value.
  /// Returns `baml.iter.Done` when iteration is finished.
  function next(self) -> Self.Item | Done throws Self.Error

  /// Creates a new iterator which calls the provided function on each element.
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the provided function will not be called unless something calls `next` on the produced iterator.
  function map<R, E2>(self, fn: (Self.Item) -> R throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    Map<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  /// Creates a new iterator which uses the provided function to decide if an element should be yielded.
  /// If the predicate returns `true` then the element is yielded, otherwise it will skip the element
  /// and try the next one.
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the provided function will not be called unless something calls `next` on the produced iterator.
  function filter<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Filter<Self.Item, Self.Error, E2> { source: self, predicate: predicate }
  }

  /// A combination of `baml.iter.Iterator.map` and `baml.iter.Iterator.filter`:
  /// If the provided function returns `null` then the element will be skipped, otherwise the
  /// produced iterator will yield the result of the provided function.
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the provided function will not be called unless something calls `next` on the produced iterator.
  function filter_map<R, E2>(self, fn: (Self.Item) -> R? throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    FilterMap<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  /// A combination of `baml.iter.Iterator.map` and `baml.iter.flatten`:
  /// For each element in the original iterator, the provided function will be called to get some iterable object.
  /// The produced iterator will then yield each element in that object's iterator and will only consume the
  /// next element of the original iterator once the object's iterator is exhausted.
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the provided function will not be called unless something calls `next` on the produced iterator.
  function flat_map<R, E2, E3>(self, fn: (Self.Item) -> Iterable<Item = R, Error = E3> throws E2) -> Iterator<Item = R, Error = Self.Error | E2 | E3> throws never {
    FlatMap<Self.Item, R, Self.Error, E2, E3> { source: self, func: fn, inner: Done {} }
  }

  /// Creates a new iterator that will only yield every `n` elements of the original iterator.
  /// For `n <= 1`, the new iterator will yield every element.
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// unless something calls `next` on the produced iterator.
  function step_by(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    StepBy<Self.Item, Self.Error> { source: self, n: n, first: true }
  }

  /// Creates a new iterator that represents the concatenation of two iterators.
  /// It will yield all elements in `self` then all elements in the iterator of `other`.
  ///
  /// Iterators are lazy (pull-based): the original iterators will not be advanced
  /// unless something calls `next` on the produced iterator.
  function chain<E2>(self, other: Iterable<Item = Self.Item, Error = E2>) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Chain<Self.Item, Self.Error, E2> { first: self, other: other.iter(), first_done: false }
  }

  /// Creates a new peekable iterator from this iterator.
  function peekable(self) -> Peekable<Self.Item, Self.Error> throws never {
    Peekable<Self.Item, Self.Error> { source: self, buffer: Done {}, has_buffer: false }
  }

  /// Creates a new iterator that yields at most the first `n` elements.
  ///
  /// Once `n` elements have been yielded the source is no longer advanced.
  /// For `n <= 0`, the new iterator yields nothing. If the source ends first,
  /// the new iterator ends with it.
  ///
  /// This is the usual way to bound an unbounded iterator such as
  /// `Repeat.new(v)` or a wide `Range`:
  ///
  /// ```
  /// baml.iter.Repeat.new(0).take(5).collect()   // [0, 0, 0, 0, 0]
  /// baml.iter.Range.new(0, 10).take(3).collect() // [0, 1, 2]
  /// ```
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// unless something calls `next` on the produced iterator.
  function take(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    Take<Self.Item, Self.Error> { source: self, remaining: n }
  }

  /// Creates a new iterator that discards the first `n` elements of the original
  /// iterator and yields the rest.
  ///
  /// For `n <= 0`, every element is yielded. If the source has fewer than `n`
  /// elements, the new iterator yields nothing.
  ///
  /// ```
  /// baml.iter.Range.new(0, 5).skip(2).collect() // [2, 3, 4]
  /// ```
  ///
  /// Iterators are lazy (pull-based): the elements are not skipped until
  /// something calls `next` on the produced iterator.
  function skip(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    Skip<Self.Item, Self.Error> { source: self, remaining: n }
  }

  /// Creates a new iterator that yields elements while `predicate` returns `true`,
  /// and stops at the first element for which it returns `false`.
  ///
  /// The element that fails the predicate is consumed from the source and is not
  /// yielded. Unlike `filter`, iteration **stops** at the first failure rather
  /// than skipping it:
  ///
  /// ```
  /// // stops at 5, so 6 and 8 are never yielded
  /// [2, 4, 5, 6, 8].iter().take_while((x: int) -> bool { x % 2 == 0 }).collect() // [2, 4]
  /// ```
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the predicate will not be called unless something calls `next` on the
  /// produced iterator.
  function take_while<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    TakeWhile<Self.Item, Self.Error, E2> { source: self, predicate: predicate, done: false }
  }

  /// Creates a new iterator that discards leading elements while `predicate`
  /// returns `true`, then yields every remaining element.
  ///
  /// The predicate is only consulted until it first returns `false`; after that
  /// every element is yielded, including ones the predicate would have matched:
  ///
  /// ```
  /// // stops skipping at 3, so the trailing 1 and 2 are kept
  /// [1, 2, 3, 1, 2].iter().skip_while((x: int) -> bool { x < 3 }).collect() // [3, 1, 2]
  /// ```
  ///
  /// Iterators are lazy (pull-based): the original iterator will not be advanced
  /// and the predicate will not be called unless something calls `next` on the
  /// produced iterator.
  function skip_while<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    SkipWhile<Self.Item, Self.Error, E2> { source: self, predicate: predicate, skipping: true }
  }

  /// Consumes the rest of this iterator and returns an array with all elements.
  ///
  /// Do not call this method on an infinite iterator as it will never terminate.
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

  /// Consumes the rest of this iterator and for each element, calls the provided function
  /// with the provided accumulator.
  ///
  /// Do not call this method on an infinite iterator as it will never terminate.
  ///
  /// ALIASES: fold
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

  /// Consumes the rest of this iterator and counts the number of elements.
  ///
  /// Do not call this method on an infinite iterator as it will never terminate.
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

  /// Consumes the rest of this iterator and calls the provided function on each element.
  /// It is generally preferred to use a `for (.. in ..)` loop instead, but
  /// this method may be useful at the end of a long chain of calls.
  function for_each<E2>(self, fn: (Self.Item) -> void throws E2) -> void throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => { fn(x); },
      }
    }
  }

  /// Returns `true` if the predicate returns `true` for any of the elements in the iterator.
  /// Short circuiting: will only consume elements up to the first `true` result.
  ///
  /// ALIASES: any
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

  /// Returns `true` if the predicate returns `true` for all of the elements in the iterator.
  /// Short circuiting: will only consume elements up to the first `false` result.
  ///
  /// ALIASES: all
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

  /// Returns the first element where the predicate returns `true`, or `null` if none do.
  /// Short circuiting: will only consume elements up to the first `true` result.
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

function flatten<R, E, E3, I extends Iterable<Item = R, Error = E3>>(
    src: Iterator<Item = I, Error = E>,
) -> Iterator<Item = R, Error = E | E3> throws never {
    Flatten<I, R, E, E3> { source: src, inner: Done {  } }
}

/// The iterator implementation for an array.
class ArrayIterator<T> {
    arr: T[],
    idx: int,

    function new(arr: T[]) -> ArrayIterator<T> throws never {
        ArrayIterator<T> { arr: arr, idx: 0 }
    }

    implements Iterable {
        type Item = T
        type Error = never

        function iter(self) -> Iterator<Item = T, Error = never> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = never

        function next(self) -> T | Done throws never {
            // Bounds-check by index, not by matching the element against null: when
            // T is itself nullable, a stored null element must be yielded, not
            // treated as the end of iteration.
            if (self.idx >= self.arr.length()) {
                return Done {  };
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
    i: int,
    max: int,
    step: int,

    function new(min: int, max: int, step: int = 1) -> Range throws never {
        Range { i: min, max: max, step: step }
    }

    implements Iterable {
        type Item = int
        type Error = never

        function iter(self) -> Iterator<Item = int, Error = never> throws never {
            self
        }
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
                Done {  }
            }
        }
    }
}

/// An iterator that returns the same value a specified number of times.
class Repeat<T> {
    value: T,
    count: int,

    /// Creates a new repeating iterator.
    /// If `count` is negative (or not specified), it will repeat forever.
    function new(value: T, count: int = -1) -> Repeat<T> throws never {
        Repeat<T> { value: value, count: count }
    }

    implements Iterable {
        type Item = T
        type Error = never

        function iter(self) -> Iterator<Item = T, Error = never> throws never {
            self
        }
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
                Done {  }
            }
        }
    }
}

class Map<T, R, E, E2> {
    source: Iterator<Item = T, Error = E>,
    func: (T) -> R throws E2,

    implements Iterable {
        type Item = R
        type Error = E | E2

        function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = R
        type Error = E | E2

        function next(self) -> R | Done throws E | E2 {
            match (self.source.next()) {
                Done => Done {  },
                let x: T => self.func(x),
            }
        }
    }
}

class Filter<T, E, E2> {
    source: Iterator<Item = T, Error = E>,
    predicate: (T) -> bool throws E2,

    implements Iterable {
        type Item = T
        type Error = E | E2

        function iter(self) -> Iterator<Item = T, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E | E2

        function next(self) -> T | Done throws E | E2 {
            while (true) {
                match (self.source.next()) {
                    Done => {
                        return Done {  };
                    },
                    let x: T => {
                        if (self.predicate(x)) {
                            return x;
                        }
                    },
                }
            }
            Done {  }
        }
    }
}

class FilterMap<T, R, E, E2> {
    source: Iterator<Item = T, Error = E>,
    func: (T) -> R? throws E2,

    implements Iterable {
        type Item = R
        type Error = E | E2

        function iter(self) -> Iterator<Item = R, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = R
        type Error = E | E2

        function next(self) -> R | Done throws E | E2 {
            while (true) {
                match (self.source.next()) {
                    Done => {
                        return Done {  };
                    },
                    let x: T => {
                        match (self.func(x)) {
                            null => {},
                            let y: R => {
                                return y;
                            },
                        }
                    },
                }
            }
            Done {  }
        }
    }
}

class FlatMap<T, R, E, E2, E3> {
    source: Iterator<Item = T, Error = E>,
    func: (T) -> Iterable<Item = R, Error = E3> throws E2,
    inner: Iterator<Item = R, Error = E3> | Done,

    implements Iterable {
        type Item = R
        type Error = E | E2 | E3

        function iter(self) -> Iterator<Item = R, Error = E | E2 | E3> throws never {
            self
        }
    }

    implements Iterator {
        type Item = R
        type Error = E | E2 | E3

        function next(self) -> R | Done throws E | E2 | E3 {
            while (true) {
                match (self.inner) {
                    let cur: Iterator<Item = R, Error = E3> => {
                        match (cur.next()) {
                            Done => {
                                self.inner = Done {  };
                            },
                            let y: R => {
                                return y;
                            },
                        }
                    },
                    Done => {
                        match (self.source.next()) {
                            Done => {
                                return Done {  };
                            },
                            let x: T => {
                                let it = self.func(x).iter();
                                self.inner = it;
                            },
                        }
                    },
                }
            }
            Done {  }
        }
    }
}

class Flatten<I extends Iterable<Item = R, Error = E3>, R, E, E3> {
    source: Iterator<Item = I, Error = E>,
    inner: Iterator<Item = R, Error = E3> | Done,

    implements Iterable {
        type Item = R
        type Error = E | E3

        function iter(self) -> Iterator<Item = R, Error = E | E3> throws never {
            self
        }
    }

    implements Iterator {
        type Item = R
        type Error = E | E3

        function next(self) -> R | Done throws E | E3 {
            while (true) {
                match (self.inner) {
                    let cur: Iterator<Item = R, Error = E3> => {
                        match (cur.next()) {
                            Done => {
                                self.inner = Done {  };
                            },
                            let y: R => {
                                return y;
                            },
                        }
                    },
                    Done => {
                        match (self.source.next()) {
                            Done => {
                                return Done {  };
                            },
                            let x: I => {
                                let it = x.iter();
                                self.inner = it;
                            },
                        }
                    },
                }
            }
            Done {  }
        }
    }
}

class Peekable<T, E> {
    source: Iterator<Item = T, Error = E>,
    buffer: T | Done,
    has_buffer: bool,

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

        function iter(self) -> Iterator<Item = T, Error = E> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E

        function next(self) -> T | Done throws E {
            if (self.has_buffer) {
                let v = self.buffer;
                self.buffer = Done {  };
                self.has_buffer = false;
                return v;
            }
            self.source.next()
        }
    }
}

class StepBy<T, E> {
    source: Iterator<Item = T, Error = E>,
    n: int,
    first: bool,

    implements Iterable {
        type Item = T
        type Error = E

        function iter(self) -> Iterator<Item = T, Error = E> throws never {
            self
        }
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
                    Done => {
                        return Done {  };
                    },
                    let x: T => {},
                }
            }
            self.source.next()
        }
    }
}

class Chain<T, E, E2> {
    first: Iterator<Item = T, Error = E>,
    other: Iterator<Item = T, Error = E2>,
    first_done: bool,

    implements Iterable {
        type Item = T
        type Error = E | E2

        function iter(self) -> Iterator<Item = T, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E | E2

        function next(self) -> T | Done throws E | E2 {
            if (!self.first_done) {
                match (self.first.next()) {
                    Done => {
                        self.first_done = true;
                    },
                    let x: T => {
                        return x;
                    },
                }
            }
            self.other.next()
        }
    }
}

class Take<T, E> {
    source: Iterator<Item = T, Error = E>,
    remaining: int,

    implements Iterable {
        type Item = T
        type Error = E

        function iter(self) -> Iterator<Item = T, Error = E> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E

        function next(self) -> T | Done throws E {
            if (self.remaining <= 0) {
                return Done {  };
            }
            // Spend a slot only once the source has actually yielded an item.
            // Decrementing first would lose a slot when `next` throws and the
            // caller retries, and would leave the count live after `Done` so a
            // later call advanced an already-finished source.
            match (self.source.next()) {
                Done => {
                    self.remaining = 0;
                },
                let x: T => {
                    self.remaining -= 1;
                    return x;
                },
            }
            Done {  }
        }
    }
}

class Skip<T, E> {
    source: Iterator<Item = T, Error = E>,
    remaining: int,

    implements Iterable {
        type Item = T
        type Error = E

        function iter(self) -> Iterator<Item = T, Error = E> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E

        function next(self) -> T | Done throws E {
            while (self.remaining > 0) {
                // Count a skip only once the source has actually yielded an
                // item, for the same reason as `Take`: a throw the caller
                // retries must not consume part of the skip count, and `Done`
                // must end the skipping phase rather than leaving it live.
                match (self.source.next()) {
                    Done => {
                        self.remaining = 0;
                        return Done {  };
                    },
                    let x: T => {
                        self.remaining -= 1;
                    },
                }
            }
            self.source.next()
        }
    }
}

class TakeWhile<T, E, E2> {
    source: Iterator<Item = T, Error = E>,
    predicate: (T) -> bool throws E2,
    done: bool,

    implements Iterable {
        type Item = T
        type Error = E | E2

        function iter(self) -> Iterator<Item = T, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E | E2

        function next(self) -> T | Done throws E | E2 {
            if (self.done) {
                return Done {  };
            }
            match (self.source.next()) {
                Done => {
                    self.done = true;
                },
                let x: T => {
                    if (self.predicate(x)) {
                        return x;
                    }
                    self.done = true;
                },
            }
            Done {  }
        }
    }
}

class SkipWhile<T, E, E2> {
    source: Iterator<Item = T, Error = E>,
    predicate: (T) -> bool throws E2,
    skipping: bool,

    implements Iterable {
        type Item = T
        type Error = E | E2

        function iter(self) -> Iterator<Item = T, Error = E | E2> throws never {
            self
        }
    }

    implements Iterator {
        type Item = T
        type Error = E | E2

        function next(self) -> T | Done throws E | E2 {
            while (self.skipping) {
                match (self.source.next()) {
                    Done => {
                        self.skipping = false;
                        return Done {  };
                    },
                    let x: T => {
                        if (!self.predicate(x)) {
                            self.skipping = false;
                            return x;
                        }
                    },
                }
            }
            self.source.next()
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
