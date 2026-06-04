/// The lazy iterator protocol for BAML, modeled after Rust's `Iterator` and
/// JS iterables. Everything in this namespace is PURE BAML: adapters are lazy
/// (constructing an adapter does no work), and consumers drive the pull loop.
///
/// `Done` is the universal end-of-iteration sentinel. It is a distinct class
/// (NOT `null`) so that an iterator can legitimately yield `null` as a value.

/// Sentinel returned by `Iterator.next` when the iterator is exhausted.
///
/// Distinct from `null` so iterators may yield `null` as a real value.
class Done {}

/// Anything that can produce an `Iterator`. Implemented by collections
/// (e.g. `T[]`) and by every `Iterator` itself (an iterator iterates to itself).
///
/// - `T` is the element type.
/// - `E` is the error type that pulling an element may throw (`never` if pure).
interface Iterable<T, E> {
  /// Produce a fresh iterator over the elements of `self`.
  function iter(self) -> Iterator<T, E> throws never
}

/// A lazy, pull-based iterator. The only required method is `next`; everything
/// else is a default method.
///
/// Adapter methods (`map`, `filter`, ...) are LAZY: they just wrap `self` in an
/// adapter class and return immediately, doing no work. Consumer methods
/// (`collect`, `reduce`, ...) drive the pull loop to completion.
interface Iterator<T, E> requires Iterable<T, E> {
  /// Pull the next element, or `Done` when exhausted. The ONLY required method.
  function next(self) -> T | Done throws E

  // ---- Adapter methods (lazy: construct an adapter, do no work) ------------

  /// Lazily transform each element with `fn`.
  function map<R, E2>(self, fn: (T) -> R throws E2) -> Iterator<R, E | E2> throws never {
    Map<T, R, E, E2> { iter: self, func: fn }
  }

  /// Lazily keep only elements for which `predicate` returns true.
  function filter<E2>(self, predicate: (T) -> bool throws E2) -> Iterator<T, E | E2> throws never {
    Filter<T, E, E2> { iter: self, predicate: predicate }
  }

  /// Lazily map-and-filter: keep each non-null result of `fn`.
  function filter_map<R, E2>(self, fn: (T) -> R? throws E2) -> Iterator<R, E | E2> throws never {
    FilterMap<T, R, E, E2> { iter: self, func: fn }
  }

  /// Lazily map each element to an `Iterable` and flatten the results.
  function flat_map<R, E2, E3>(self, fn: (T) -> Iterable<R, E3> throws E2) -> Iterator<R, E | E2 | E3> throws never {
    FlatMap<T, R, E, E2, E3> { iter: self, func: fn, inner: Done {} }
  }

  /// Wrap in an iterator that supports one-element lookahead via `peek`.
  function peekable(self) -> Peekable<T, E> throws never {
    Peekable<T, E> { iter: self, buffer: Done {}, has_buffer: false }
  }

  /// Lazily yield every `n`-th element (the first element, then skip `n - 1`).
  function step_by(self, n: int) -> Iterator<T, E> throws never {
    StepBy<T, E> { iter: self, n: n, first: true }
  }

  /// Lazily yield all of `self`, then all of `other`.
  function chain(self, other: Iterator<T, E>) -> Iterator<T, E> throws never {
    Chain<T, E> { iter: self, other: other, first_done: false }
  }

  // ---- Consumer methods (drive the pull loop) ------------------------------

  /// Collect all remaining elements into an array.
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

  /// Fold all elements into a single accumulator value.
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

  /// Count the remaining elements, consuming the iterator.
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

  /// Return true on the first element matching `predicate` (short-circuit).
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

  /// Return false on the first element failing `predicate` (short-circuit).
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

  /// Return the first element matching `predicate`, or `null` (short-circuit).
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

/// Flatten an iterator whose elements are themselves `Iterable` into a single
/// lazy iterator over the inner elements.
///
/// `flatten` is a free function (not an `Iterator` default method) because the
/// protocol cannot express the "elements are iterable" constraint on the
/// receiver's element type without a receiver bound. Call it as
/// `baml.iter.flatten(some_iter_of_iterables)`.
function flatten<R, E, E3>(src: Iterator<Iterable<R, E3>, E>) -> Iterator<R, E | E3> throws never {
  Flatten<R, E, E3> { iter: src, inner: Done {} }
}
