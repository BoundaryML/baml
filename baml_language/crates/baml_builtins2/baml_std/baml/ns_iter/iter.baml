/// The sentinel `baml.iter.Iterator.next` returns when there are no elements
/// left.
///
/// `next` returns `Self.Item | Done` rather than `Self.Item?` because an
/// iterator's item type may itself be nullable: an iterator over `string?` has
/// to be able to yield a stored `null` without that being read as the end of
/// the sequence. A separate sentinel *type* keeps the two apart, and `match`
/// tells them apart by type rather than by value.
///
/// It carries no data — every `Done` is interchangeable — so it is written
/// `Done {}` at a construction site.
class Done {
}

/// Implemented by types that can be iterated over.
///
/// This is what `for (x in xs)` asks for: a container implements `Iterable` and
/// hands out an iterator. It is Rust's
/// [`IntoIterator`](https://doc.rust-lang.org/std/iter/trait.IntoIterator.html),
/// and plays the part of Python's `__iter__` or JavaScript's
/// `[Symbol.iterator]`. Because `baml.iter.Iterator` in turn requires
/// `Iterable`, an iterator is itself iterable and can be passed anywhere an
/// `Iterable` is wanted.
///
/// `T[]` and `string` implement it at the bottom of this file.
interface Iterable {
  /// The type of the elements yielded by the iterator.
  type Item
  /// The type of error iterating can throw.
  ///
  /// Defaults to `never`, so an implementation that cannot fail may leave it
  /// out entirely.
  type Error = never

  /// Creates an iterator over the value.
  ///
  /// The return type is the interface rather than a concrete class, so the
  /// implementor picks its own iterator type: `T[]` returns a
  /// `baml.iter.ArrayIterator`, while every iterator in this file returns
  /// `self`.
  ///
  /// That last part is worth remembering. Calling `iter` on something that is
  /// already an iterator hands back the same, shared iterator. It does not
  /// rewind a partly-consumed one, and two callers will pull from the same
  /// cursor.
  function iter(self) -> Iterator<Item = Self.Item, Error = Self.Error> throws never
}

/// Implemented by types that produce a sequence of values, often from some
/// container.
///
/// `next` is the only method without a default, so writing a new iterator means
/// writing one method; everything else here is built from it. `requires
/// Iterable<...>` makes every iterator iterable as itself, the same
/// relationship Rust gets from its blanket `IntoIterator for I: Iterator`.
///
/// The adapters (`map`, `filter`, `take`, …) are lazy, like Rust's
/// [`Iterator`](https://doc.rust-lang.org/std/iter/trait.Iterator.html),
/// Python's `itertools`, and JavaScript's iterator helpers: each returns a
/// small class that holds the source and does no work — not even calling the
/// function you passed it — until something calls `next` on it.
///
/// The consumers are what drive a chain, and so are where the source's errors
/// surface. `collect`, `reduce`, `count` and `for_each` run it to exhaustion,
/// and so never return on an infinite iterator; bound it with `take` first.
/// `some`, `every` and `find` short circuit, and are safe on an infinite
/// iterator so long as some element eventually settles the answer.
///
/// Adapters that take a function widen the error type: a chain's `Error` is the
/// union of the source's and of every callback's, so one fallible step makes
/// the whole chain fallible and says so in its type. Python and JavaScript
/// track nothing of the sort — there a throwing callback simply propagates.
interface Iterator requires Iterable<Item = Self.Item, Error = Self.Error> {
  /// The type of the elements yielded by the iterator.
  type Item
  /// The type of error iterating can throw.
  ///
  /// Defaults to `never`. An adapter's `Error` is the union of its source's and
  /// of any callback's, which is how a fallible step propagates down a chain.
  type Error = never

  /// Advances the iterator and returns the next element, or `baml.iter.Done`
  /// once there are none left.
  ///
  /// This is the one method an implementor must write; the rest of the
  /// interface is defined in terms of it.
  ///
  /// Calling `next` again after it has returned `Done` is allowed. The leaf
  /// iterators here (`ArrayIterator`, `Range`, `Repeat`) and the adapters that
  /// latch (`take`, `take_while`) keep returning `Done`; the other adapters
  /// just ask their source again, so what happens is whatever the source does.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if an error occurs while iterating. Depends on the
  ///   implementation.
  function next(self) -> Self.Item | Done throws Self.Error

  /// Returns an iterator that applies `fn` to each element — Python's `map`,
  /// JavaScript's `Iterator.prototype.map`.
  ///
  /// `fn` is applied exactly once per element that is actually pulled, in
  /// order, so a chain that ends early never touches the elements it skipped.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Map` yielding `R`, of the same length as `self`.
  function map<R, E2>(self, fn: (Self.Item) -> R throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    Map<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  /// Returns an iterator over the elements for which `predicate` is `true`, in
  /// order.
  ///
  /// A rejected element is skipped, not a stopping point — use
  /// `baml.iter.Iterator.take_while` to stop at the first failure instead.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Filter` yielding the kept elements.
  function filter<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Filter<Self.Item, Self.Error, E2> { source: self, predicate: predicate }
  }

  /// Returns an iterator over the non-null results of `fn`: `map` and `filter`
  /// in one pass.
  ///
  /// `null` is the skip signal, so the produced item type `R` is the
  /// non-optional one. An iterator whose own items you want to keep as `null`
  /// cannot be expressed this way — filter and map it in two steps instead.
  ///
  /// # Returns
  ///
  /// A `baml.iter.FilterMap` yielding the non-null results, in order.
  function filter_map<R, E2>(self, fn: (Self.Item) -> R? throws E2) -> Iterator<Item = R, Error = Self.Error | E2> throws never {
    FilterMap<Self.Item, R, Self.Error, E2> { source: self, func: fn }
  }

  /// Returns an iterator over the elements of the iterables `fn` produces, laid
  /// end to end.
  ///
  /// Each inner sequence is drained before the next outer element is pulled,
  /// and one that is empty contributes nothing. Three error types meet here —
  /// the source's, `fn`'s `E2`, and the inner iterables' `E3` — and the result
  /// throws all of them.
  ///
  /// # Returns
  ///
  /// A `baml.iter.FlatMap` yielding `R`.
  function flat_map<R, E2, E3>(self, fn: (Self.Item) -> Iterable<Item = R, Error = E3> throws E2) -> Iterator<Item = R, Error = Self.Error | E2 | E3> throws never {
    FlatMap<Self.Item, R, Self.Error, E2, E3> { source: self, func: fn, inner: Done {} }
  }

  /// Returns an iterator over every `n`th element, starting with the first.
  ///
  /// The elements kept are those at positions `0`, `n`, `2n`, and so on. The
  /// skipped ones are still pulled from the source — and so may still throw —
  /// they are simply discarded.
  ///
  /// # Parameters
  ///
  /// - `n`: how far to advance between yielded elements. Values below `1` mean
  ///   "advance by one", i.e. yield everything.
  ///
  /// # Returns
  ///
  /// A `baml.iter.StepBy`.
  function step_by(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    StepBy<Self.Item, Self.Error> { source: self, n: n, first: true }
  }

  /// Returns an iterator over all of `self`'s elements followed by all of
  /// `other`'s.
  ///
  /// `other.iter()` is called right away, when the chain is built; only the
  /// advancing is deferred. The two sides may have different error types, and
  /// the result throws either.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Chain`.
  function chain<E2>(self, other: Iterable<Item = Self.Item, Error = E2>) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    Chain<Self.Item, Self.Error, E2> { first: self, other: other.iter(), first_done: false }
  }

  /// Returns an iterator with one element of look-ahead.
  ///
  /// Unlike the other adapters this returns the concrete `baml.iter.Peekable`
  /// class rather than the `Iterator` interface, because `peek` lives on the
  /// class: an existential would hide it.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Peekable` yielding exactly what `self` yields, plus a `peek`
  /// that reads the next element without consuming it.
  function peekable(self) -> Peekable<Self.Item, Self.Error> throws never {
    Peekable<Self.Item, Self.Error> { source: self, buffer: Done {}, has_buffer: false }
  }

  /// Returns an iterator over at most the first `n` elements.
  ///
  /// Once `n` elements have been yielded the source is no longer advanced, and
  /// if the source ends first this ends with it. For `n <= 0` it yields
  /// nothing. A slot is spent only once the source has actually yielded an
  /// element, so a source that throws and is then retried does not silently
  /// lose one.
  ///
  /// This is the usual way to bound an unbounded iterator such as
  /// `Repeat.new(v)` or a wide `Range`.
  ///
  /// # Parameters
  ///
  /// - `n`: the greatest number of elements to yield.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Take` yielding at most `n` elements.
  ///
  /// # Examples
  ///
  /// ```baml
  /// baml.iter.Repeat.new(0).take(5).collect()   // [0, 0, 0, 0, 0]
  /// baml.iter.Range.new(0, 10).take(3).collect() // [0, 1, 2]
  /// ```
  function take(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    Take<Self.Item, Self.Error> { source: self, remaining: n }
  }

  /// Returns an iterator over everything after the first `n` elements.
  ///
  /// For `n <= 0` every element is yielded; a source with fewer than `n`
  /// elements yields nothing. The skipped elements are pulled from the source
  /// and discarded — they are not free, and they can still throw. A skip is
  /// counted only once the source has actually yielded, so a throw the caller
  /// retries does not consume part of the count.
  ///
  /// # Parameters
  ///
  /// - `n`: how many leading elements to discard.
  ///
  /// # Returns
  ///
  /// A `baml.iter.Skip`.
  ///
  /// # Examples
  ///
  /// ```baml
  /// baml.iter.Range.new(0, 5).skip(2).collect() // [2, 3, 4]
  /// ```
  function skip(self, n: int) -> Iterator<Item = Self.Item, Error = Self.Error> throws never {
    Skip<Self.Item, Self.Error> { source: self, remaining: n }
  }

  /// Returns an iterator over the leading run of elements for which
  /// `predicate` is `true`.
  ///
  /// The element that fails the predicate is consumed from the source and is
  /// not yielded. Unlike `filter`, iteration **stops** at the first failure
  /// rather than skipping it, and the stop latches: the source is never
  /// advanced again, however often `next` is called.
  ///
  /// # Returns
  ///
  /// A `baml.iter.TakeWhile`.
  ///
  /// # Examples
  ///
  /// ```baml
  /// // stops at 5, so 6 and 8 are never yielded
  /// [2, 4, 5, 6, 8].iter().take_while((x: int) -> bool { x % 2 == 0 }).collect() // [2, 4]
  /// ```
  function take_while<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    TakeWhile<Self.Item, Self.Error, E2> { source: self, predicate: predicate, done: false }
  }

  /// Returns an iterator over everything from the first element `predicate`
  /// rejects onwards.
  ///
  /// That first rejected element is itself yielded — it is the start of the
  /// kept run, not a discarded boundary. The predicate is not consulted again
  /// afterwards, so later elements it would have matched are still yielded.
  ///
  /// # Returns
  ///
  /// A `baml.iter.SkipWhile`.
  ///
  /// # Examples
  ///
  /// ```baml
  /// // stops skipping at 3, so the trailing 1 and 2 are kept
  /// [1, 2, 3, 1, 2].iter().skip_while((x: int) -> bool { x < 3 }).collect() // [3, 1, 2]
  /// ```
  function skip_while<E2>(self, predicate: (Self.Item) -> bool throws E2) -> Iterator<Item = Self.Item, Error = Self.Error | E2> throws never {
    SkipWhile<Self.Item, Self.Error, E2> { source: self, predicate: predicate, skipping: true }
  }

  /// Consumes the rest of this iterator into an array, in order.
  ///
  /// The *rest*: elements already pulled from a partly-consumed iterator are
  /// not included, and an already-exhausted iterator collects to `[]`.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails. The elements gathered so
  ///   far are lost, since the array is only returned on success.
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

  /// Folds every remaining element into an accumulator, left to right.
  ///
  /// The accumulator starts at `initial` and each call to `fn` replaces it with
  /// the result; `initial` is what comes back for an empty iterator. Because
  /// the seed is required and its type `A` is independent of the item type,
  /// this is the operation Rust calls `Iterator::fold` and Python calls
  /// `functools.reduce` — not Rust's `reduce`.
  ///
  /// # Parameters
  ///
  /// - `fn`: combines the accumulator so far with the next element.
  /// - `initial`: the starting accumulator, and the result for an empty
  ///   iterator.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
  /// - `E2` if `fn` throws. The fold stops there; the partial accumulator is
  ///   not recoverable.
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

  /// Consumes the rest of this iterator and returns how many elements there
  /// were, `0` if it was already exhausted.
  ///
  /// Every element is actually pulled — there is no size hint to short circuit
  /// with — so counting a chain runs the whole chain, callbacks included.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
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

  /// Calls `fn` on each remaining element, in order, for its side effects.
  ///
  /// A `for (.. in ..)` loop is generally preferable; this is for the end of a
  /// long chain of calls. It runs to exhaustion — there is no way for `fn` to
  /// stop it early. Use `baml.iter.Iterator.some` or a `for` loop with a
  /// `break` when you need to.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
  /// - `E2` if `fn` throws, which ends the traversal at that element.
  function for_each<E2>(self, fn: (Self.Item) -> void throws E2) -> void throws Self.Error | E2 {
    while (true) {
      match (self.next()) {
        Done => { break; },
        let x: Self.Item => { fn(x); },
      }
    }
  }

  /// Whether `predicate` holds for any remaining element — Python's `any`,
  /// JavaScript's `some`.
  ///
  /// Short circuits at the first `true`, leaving the iterator positioned just
  /// after the matching element so it can be inspected further. An empty
  /// iterator gives `false`, the empty disjunction.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
  /// - `E2` if `predicate` throws.
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

  /// Whether `predicate` holds for every remaining element — Python's `all`,
  /// JavaScript's `every`.
  ///
  /// Short circuits at the first `false`. An empty iterator gives `true` — the
  /// empty conjunction — which is worth remembering when the iterator may have
  /// been exhausted already.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
  /// - `E2` if `predicate` throws.
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

  /// The first remaining element for which `predicate` is `true`, or `null` if
  /// the iterator runs out first.
  ///
  /// Short circuits: the matching element is consumed, and the iterator is left
  /// positioned just after it.
  ///
  /// The result is `Self.Item?`, so — unlike `next`, which uses the distinct
  /// `baml.iter.Done` sentinel — an iterator whose items are themselves
  /// nullable cannot tell a found `null` from no match. Use
  /// `baml.iter.Iterator.filter` followed by `next` when that distinction
  /// matters.
  ///
  /// # Throws
  ///
  /// - `Self.Error` if advancing the iterator fails.
  /// - `E2` if `predicate` throws.
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

/// Flattens an iterator of iterables into an iterator over their elements.
///
/// Each element of `src` is iterated to exhaustion before the next one is
/// pulled, and an inner sequence that is empty contributes nothing and is
/// skipped over silently. Only one level is removed — flattening an iterator of
/// iterators of arrays leaves the arrays intact. Reach for
/// `baml.iter.Iterator.flat_map` instead when the elements still have to be
/// turned into iterables first.
///
/// # Returns
///
/// A `baml.iter.Flatten` yielding the inner elements laid end to end. It throws
/// both the outer iterator's errors and the inner ones'.
function flatten<R, E, E3, I extends Iterable<Item = R, Error = E3>>(
    src: Iterator<Item = I, Error = E>,
) -> Iterator<Item = R, Error = E | E3> throws never {
    Flatten<I, R, E, E3> { source: src, inner: Done {  } }
}

/// The iterator implementation for an array.
///
/// Walks `arr` from `idx` forward, one element per `next`. This is what
/// `T[].iter()` — and therefore `for (x in xs)` over an array — produces.
///
/// The array is referenced, not copied, so changes made to it while iterating
/// are visible: the length is re-read on every `next`, so appending extends the
/// iteration and truncating below `idx` ends it.
class ArrayIterator<T> {
    /// The array being walked, held by reference.
    arr: T[],
    /// The position of the next element to yield.
    idx: int,

    /// Creates an iterator positioned at the start of `arr`, which is not
    /// copied.
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

/// An iterator over the integers from `min` up to but excluding `max`.
///
/// The upper bound is exclusive, so `Range.new(0, 3)` yields `0`, `1`, `2`.
/// With a `step` other than `1` it yields `min`, `min + step`, `min + 2*step`,
/// … while the value is still below `max`; the bound is a stopping condition,
/// not a value that has to be landed on.
///
/// The end test is always `i < max`, so with `min < max` a non-positive `step`
/// never terminates — see `new` — and there is no descending range.
///
/// Advancing past `int.max_value()` raises `baml.panics.IntegerOverflow`, as
/// the underlying `+` would.
class Range {
    /// The next value to yield.
    i: int,
    /// The exclusive upper bound. Iteration stops once `i` reaches it.
    max: int,
    /// Added to `i` after each yielded value.
    step: int,

    /// Creates a range that starts at `min` and adds `step` after each item,
    /// stopping at the first value that reaches or passes `max`.
    ///
    /// If `min >= max` the range is empty whatever `step` is. Otherwise a
    /// non-positive `step` never reaches `max` — zero repeats `min`, negative
    /// descends — so the range is infinite; pair it with `take` if that is
    /// what you want.
    ///
    /// # Parameters
    ///
    /// - `min`: the first value yielded, provided it is already below `max`.
    /// - `max`: the exclusive upper bound. A `max` at or below `min` yields
    ///   nothing.
    /// - `step`: the increment between values; defaults to `1`.
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
///
/// It yields the *same* value, not a copy of it: repeating a class instance
/// hands out one shared instance, so mutating a yielded element is visible
/// through every other one. Pair it with `map` when each round needs its own.
///
/// A negative `count` means "forever", which is the default — bound it with
/// `baml.iter.Iterator.take` before consuming it.
class Repeat<T> {
    /// The value handed out on each `next`.
    value: T,
    /// How many yields remain, or a negative number for "unbounded".
    count: int,

    /// Creates a repeating iterator.
    ///
    /// # Parameters
    ///
    /// - `value`: the value to yield on every round.
    /// - `count`: how many times to yield it. `0` yields nothing; any negative
    ///   value, including the default, repeats without end.
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

/// The iterator `baml.iter.Iterator.map` returns.
///
/// Each `next` pulls one element from `source` and hands it to `func`, so the
/// result has exactly as many elements as the source and `func` runs once per
/// element pulled.
///
/// Like the other adapter classes here it is built by its method — `map`
/// returns it behind the `Iterator` interface — and there is rarely a reason to
/// name it directly.
class Map<T, R, E, E2> {
    /// The iterator being mapped over.
    source: Iterator<Item = T, Error = E>,
    /// Applied to each element to produce the yielded value.
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

/// The iterator `baml.iter.Iterator.filter` returns.
///
/// Each `next` pulls from `source` until `predicate` accepts an element or the
/// source runs out, so a single `next` may consume many elements and call the
/// predicate many times.
class Filter<T, E, E2> {
    /// The iterator being filtered.
    source: Iterator<Item = T, Error = E>,
    /// Decides whether an element is yielded (`true`) or skipped.
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

/// The iterator `baml.iter.Iterator.filter_map` returns.
///
/// Each `next` pulls from `source` until `func` returns a non-null result or
/// the source runs out, yielding that result. A `null` from `func` skips the
/// element rather than being yielded.
class FilterMap<T, R, E, E2> {
    /// The iterator being filtered and mapped.
    source: Iterator<Item = T, Error = E>,
    /// Produces the value to yield, or `null` to skip the element.
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

/// The iterator `baml.iter.Iterator.flat_map` returns.
///
/// Keeps at most one inner iterator live at a time: while `inner` is an
/// iterator, elements come from it; when it runs out, `inner` is reset to
/// `Done` and the next outer element is mapped to a fresh one. Outer elements
/// that map to empty sequences are passed over in the same loop, so a single
/// `next` may advance the source several times.
class FlatMap<T, R, E, E2, E3> {
    /// The outer iterator whose elements are mapped to iterables.
    source: Iterator<Item = T, Error = E>,
    /// Turns an outer element into the iterable to yield from.
    func: (T) -> Iterable<Item = R, Error = E3> throws E2,
    /// The inner iterator currently being drained, or `Done` when the next
    /// outer element still has to be pulled.
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

/// The iterator `baml.iter.flatten` returns.
///
/// The same one-inner-iterator-at-a-time structure as `baml.iter.FlatMap`,
/// minus the mapping step: each outer element is already iterable, so it is
/// asked for its own iterator directly.
class Flatten<I extends Iterable<Item = R, Error = E3>, R, E, E3> {
    /// The outer iterator, whose elements are themselves iterable.
    source: Iterator<Item = I, Error = E>,
    /// The inner iterator currently being drained, or `Done` when the next
    /// outer element still has to be pulled.
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

/// The iterator `baml.iter.Iterator.peekable` returns.
///
/// Wraps a source with room for one look-ahead element, so a caller can decide
/// what to do with an element before committing to consuming it. The buffer
/// holds at most one item, filled by `peek` and drained by the following
/// `next`.
///
/// `has_buffer` is a separate flag rather than a `Done` in `buffer` because
/// `Done` is also a legitimate buffered answer: a `peek` at the end of the
/// source has to be remembered as "peeked, and the answer was `Done`".
class Peekable<T, E> {
    /// The iterator being wrapped.
    source: Iterator<Item = T, Error = E>,
    /// The look-ahead slot: the element a `peek` pulled, or `Done` if the peek
    /// hit the end. Meaningful only while `has_buffer` is `true`.
    buffer: T | Done,
    /// Whether `buffer` currently holds a peeked answer.
    has_buffer: bool,

    /// Returns the element the next `next` will return, or `baml.iter.Done` if
    /// there is none, without consuming it.
    ///
    /// The item is buffered, so the following `next` returns the same one and
    /// repeated `peek` calls do not advance the source.
    ///
    /// # Throws
    ///
    /// - `E` if advancing the source fails. Nothing is buffered in that case,
    ///   so a retry pulls again rather than replaying the failure.
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

/// The iterator `baml.iter.Iterator.step_by` returns.
///
/// The first `next` simply forwards to the source; every later one discards
/// `n - 1` elements before forwarding, which is what makes the yielded
/// positions `0`, `n`, `2n`, … The discarded elements are still pulled, so they
/// can still throw.
class StepBy<T, E> {
    /// The iterator being stepped through.
    source: Iterator<Item = T, Error = E>,
    /// The stride: how far to advance between yielded elements.
    n: int,
    /// Whether the next pull is the first one, which is yielded without
    /// skipping ahead.
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

/// The iterator `baml.iter.Iterator.chain` returns.
///
/// Drains `first`, then `other`. The switch is one-way: once `first` has
/// reported the end it is never asked again, even if it could later produce
/// more.
class Chain<T, E, E2> {
    /// The iterator drained first.
    first: Iterator<Item = T, Error = E>,
    /// The iterator drained once `first` is exhausted.
    other: Iterator<Item = T, Error = E2>,
    /// Whether `first` has already reported the end.
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

/// The iterator `baml.iter.Iterator.take` returns.
///
/// Yields until either the budget or the source runs out, then latches: once
/// `remaining` reaches zero the source is never advanced again. A slot is spent
/// only after the source has actually yielded, so a `next` that throws and is
/// retried does not cost one.
class Take<T, E> {
    /// The iterator being bounded.
    source: Iterator<Item = T, Error = E>,
    /// How many more elements may be yielded. Zero means finished.
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

/// The iterator `baml.iter.Iterator.skip` returns.
///
/// The first `next` discards `remaining` elements before yielding anything;
/// after that it forwards to the source unchanged. As with `baml.iter.Take`, a
/// skip is counted only once the source has actually yielded, so a `next` that
/// throws and is retried does not consume part of the count.
class Skip<T, E> {
    /// The iterator whose leading elements are discarded.
    source: Iterator<Item = T, Error = E>,
    /// How many more elements to discard before yielding. Zero means the
    /// skipping phase is over.
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

/// The iterator `baml.iter.Iterator.take_while` returns.
///
/// Yields the leading run of elements the predicate accepts, then latches: the
/// element that fails is consumed from the source but not yielded, and the
/// source is never advanced again.
class TakeWhile<T, E, E2> {
    /// The iterator being truncated.
    source: Iterator<Item = T, Error = E>,
    /// Consulted on each element; the first `false` ends the iteration.
    predicate: (T) -> bool throws E2,
    /// Whether the predicate has already failed (or the source has ended), in
    /// which case only `Done` is produced from here on.
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

/// The iterator `baml.iter.Iterator.skip_while` returns.
///
/// Discards the leading run of elements the predicate accepts, yields the first
/// one it rejects, and forwards everything after that without consulting the
/// predicate again.
class SkipWhile<T, E, E2> {
    /// The iterator whose leading elements are discarded.
    source: Iterator<Item = T, Error = E>,
    /// Consulted only during the skipping phase; the first `false` ends it.
    predicate: (T) -> bool throws E2,
    /// Whether the leading run is still being discarded.
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

  /// Returns a fresh `baml.iter.ArrayIterator` over the array's elements in
  /// order, positioned at the start — so an array, unlike an iterator, can be
  /// walked more than once.
  ///
  /// This is what backs `for (x in xs)` over an array and what lets an array be
  /// passed anywhere an `Iterable` is wanted. Infallible: reading an array
  /// cannot fail.
  function iter(self) -> Iterator<Item = T, Error = never> throws never {
    ArrayIterator.new(self)
  }
}

implements Iterable for string {
  type Item = string
  type Error = never

  /// Yields the string's Unicode code points as one-character strings — BAML
  /// has no character type, so the item type is `string`.
  ///
  /// The unit is the code point, not the byte and not the grapheme cluster:
  /// `"é😀"` yields two items, but an `é` written as a base letter plus a
  /// combining accent yields two items of its own.
  ///
  /// This is the one `Iterable` here that is not lazy. It delegates to
  /// `string.chars()`, which materializes the whole `string[]` up front, so
  /// iterating a string costs its full length even if only the first character
  /// is pulled.
  function iter(self) -> Iterator<Item = string, Error = never> throws never {
    self.chars().iter()
  }
}
