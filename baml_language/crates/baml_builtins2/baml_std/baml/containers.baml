/// An ordered, growable collection of elements of type T.
class Array<T> {
    /// Builds a new array of `length` elements, each equal to `value`.
    ///
    /// This is the idiomatic way to allocate a runtime-sized, pre-initialized
    /// buffer — Fenwick trees, DP tables, adjacency lists — without a manual
    /// `while`/`push` loop. (Remember a 1-indexed structure needs `length + 1`
    /// slots.)
    ///
    /// # Parameters
    /// - `length`: the number of elements to create. A negative or zero `length`
    ///   produces an empty array.
    /// - `value`: the value stored in every slot.
    ///
    /// # Warning
    /// `Array.filled` reuses the exact same `value` for every slot. For reference
    /// types (arrays, maps, class instances), every slot aliases the same object.
    /// Mutating one slot mutates all slots.
    ///
    /// To get independent per-slot values, use `Array.generate`, which calls a
    /// factory once per index:
    /// ```
    /// Array.generate(3, (i: int) -> int[] { [] })   // three independent []s
    /// ```
    ///
    /// # Returns
    /// A fresh `T[]` of length `max(length, 0)` with every element equal to
    /// `value`.
    ///
    /// Never throws.
    ///
    /// # Examples
    /// ```
    /// Array.filled(3, 0)     // [0, 0, 0]
    /// Array.filled(0, 0)     // []
    /// Array.filled(-1, 0)    // []
    /// Array.filled(2, "x")   // ["x", "x"]
    /// Array.filled(3, [])    // aliases the same inner array in all 3 slots
    /// ```
    function filled(length: int, value: T) -> T[] throws never {
        $rust_function
    }

    /// Builds a new array of `length` elements by calling `f` once per index.
    ///
    /// `f` is invoked with each index `0, 1, ..., length - 1` in order, and its
    /// result is stored in that slot. Unlike `Array.filled`, which reuses one
    /// shared value, `generate` produces an **independent value per slot** — the
    /// alias-free way to build runtime-sized buffers of reference types (rows of a
    /// grid, per-slot maps or class instances). It mirrors JavaScript's
    /// `Array.from({ length }, f)` and a Python list comprehension.
    ///
    /// # Parameters
    /// - `length`: the number of elements to create. A negative or zero `length`
    ///   produces an empty array and never calls `f`.
    /// - `f`: called once per index to produce that slot's value. Any error it
    ///   throws propagates to the caller, halting generation.
    ///
    /// # Returns
    /// A fresh `T[]` of length `max(length, 0)` whose element `i` is `f(i)`.
    ///
    /// # Examples
    /// ```
    /// Array.generate(3, (i: int) -> int { i * i })      // [0, 1, 4]
    /// Array.generate(0, (i: int) -> int { i })          // []
    /// // A 2D grid with independent rows (mutating one row leaves the rest):
    /// Array.generate(rows, (r: int) -> int[] { Array.filled(cols, 0) })
    /// ```
    function generate<E>(length: int, f: (int) -> T throws E) -> T[] throws E {
        let out: T[] = [];
        let i = 0;
        while (i < length) {
            out.push(f(i));
            i += 1;
        }
        out
    }

    /// Returns the number of elements in the array.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns the element at `index`, or `null` if out of bounds.
    ///
    /// Negative indices count from the end: `-1` is the last element.
    ///
    /// # Examples
    /// ```
    /// [10, 20, 30].at(0)    // 10
    /// [10, 20, 30].at(-1)   // 30
    /// [10, 20, 30].at(99)   // null
    /// [10, 20, 30].at(-99)  // null
    /// ```
    function at(self, index: int) -> T? throws never {
        $rust_function
    }

    /// Appends `item` to the end of the array. Returns the new length. Mutates `self`.
    //baml:mut_self
    function push(self, item: T) -> int throws never {
        $rust_function
    }

    /// Removes and returns the last element, or `null` if the array is empty. Mutates `self`.
    //baml:mut_self
    function pop(self) -> T? throws never {
        $rust_function
    }

    /// Removes and returns the element at `index`, or `null` if out of bounds.
    /// Mutates `self` in place.
    ///
    /// Negative indices count from the end: `-1` is the last element.
    ///
    /// `O(n)` because subsequent elements shift backward. For `O(1)` removal from
    /// the back use `pop`.
    ///
    /// # Examples
    /// ```
    /// let xs = [10, 20, 30];
    /// xs.remove_at(1)   // 20   xs is now [10, 30]
    /// xs.remove_at(-1)  // 30   removes the last element; xs is now [10]
    /// xs.remove_at(99)  // null
    /// ```
    //baml:mut_self
    function remove_at(self, index: int) -> T? throws never {
        $rust_function
    }

    /// Returns a new array with the elements of `self` followed by the elements of `other`.
    function concat(self, other: T[]) -> T[] throws never {
        $rust_function
    }

    /// Calls `fn` once for each element in the array.
    function for_each<E>(self, fn: (T) -> void throws E) -> void throws E {
        for (let x in self.slice(0, self.length())) {
            fn(x);
        }
    }

    /// Returns a new array containing the elements for which `predicate` returns
    /// true. The predicate is called once per element, left-to-right. Does not
    /// mutate `self`.
    //baml:mut_vm
    //baml:may_yield
    function filter<E>(self, predicate: (T) -> bool throws E) -> T[] throws E {
        $rust_function
    }

    /// Returns a new array containing the non-null results of `fn` applied to
    /// every element. The function is called once per element, left-to-right.
    /// Does not mutate `self`. For a lazy version, use `iter().filter_map(fn)`.
    function filter_map<U, E>(self, fn: (T) -> U? throws E) -> U[] throws E {
        let out: U[] = [];
        for (let x in self.slice(0, self.length())) {
            match (fn(x)) {
                null => {},
                let y: U => {
                    out.push(y);
                },
            }
        }
        out
    }

    /// Returns a new array with the elements in reverse order. Does not mutate `self`.
    function reverse(self) -> T[] throws never {
        $rust_function
    }

    // `sort()` is no longer a method of `Array<T>`: it is provided by the
    // `Sortable` interface (`baml/comparable.baml`), blanket-implemented for
    // `T[]` where `T implements Comparable`. This makes `sort()` reject
    // non-orderable element types at compile time (with a `sort_by` pointer)
    // and infallible (`throws never`) for `int`/`bigint`/`string`, instead of
    // the old unconditional `throws InvalidArgument`. `sort_by`/`sort_by_key`
    // remain here.

    /// Sorts this array in place using a comparator, then returns `self`.
    ///
    /// The comparator follows the JavaScript convention: negative means the
    /// first argument sorts before the second, zero preserves relative order, and
    /// positive means it sorts after the second.
    ///
    /// Stable: elements the comparator reports as equal keep their relative
    /// order. If the comparator throws, `self` is left in its pre-sort state —
    /// the sorted result is written back only once the whole sort completes.
    /// Comparator calls run sequentially in a deterministic merge order. Their
    /// exact order is an implementation detail; comparators should not depend on
    /// a particular comparison sequence.
    //baml:mut_vm
    //baml:may_yield
    //baml:mut_self
    function sort_by<E>(self, compare: (T, T) -> int throws E) -> T[] throws E {
        $rust_function
    }

    /// Sorts this array in place by keys computed once per element, then returns
    /// `self`.
    ///
    /// Each key is computed exactly once, left-to-right, and the elements are
    /// ordered by their keys' natural order (`Comparable.compare`). The sort is
    /// stable: elements with equal keys keep their relative order. If computing a
    /// key — or comparing two keys — throws, `self` is left in its pre-sort
    /// state.
    ///
    /// The key type must be `Comparable`; `U.CompareError` (the key's comparison
    /// error) propagates to the caller alongside the key callback's own `E`. For
    /// `int`/`bigint`/`string` keys that is `never`, so the only error is `E`.
    //baml:mut_self
    function sort_by_key<U extends Comparable, E>(
        self,
        key: (T) -> U throws E,
    ) -> T[] throws E | U.CompareError {
        let n = self.length();
        if (n <= 1) {
            return self;
        }
        let items = self.slice(0, n);
        // Compute each key exactly once, left to right.
        let keys: U[] = [];
        for (let x in items) {
            keys.push(key(x));
        }
        // Stably sort the positions `[0, n)` by their keys. `sort_by` is stable, so
        // equal keys keep their original relative order with no explicit tie-break.
        let order: int[] = [];
        let p = 0;
        while (p < n) {
            order.push(p);
            p += 1;
        }
        order.sort_by((i: int, j: int) -> int throws U.CompareError {
            match (keys.at(i)) {
                null => 0,
                let ki: U => match (keys.at(j)) {
                    null => 0,
                    let kj: U => ki.compare(kj),
                },
            }
        });
        // Materialize the reordered elements, then write them back in place.
        let sorted: T[] = [];
        for (let i in order) {
            match (items.at(i)) {
                null => {},
                let item: T => {
                    sorted.push(item);
                },
            }
        }
        self.clear();
        for (let s in sorted) {
            self.push(s);
        }
        return self;
    }

    /// Returns a sub-array from index `start` (inclusive) to `end` (exclusive).
    ///
    /// Negative indices count from the end. Out-of-range indices are clamped, and
    /// an `end` that resolves at or before `start` yields an empty array.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3, 4].slice(1, 3)   // [2, 3]
    /// [1, 2, 3].slice(-2, 3)     // [2, 3]
    /// [1, 2, 3].slice(2, 1)      // []
    /// ```
    function slice(self, start: int, end: int) -> T[] throws never {
        $rust_function
    }

    /// Joins all elements into a string, separated by `separator`. Each element is converted to a string via its `to_string` representation.
    function join(self, separator: string) -> string throws never {
        match (self) {
            [] => "",
            [let first, ..let rest] => {
                let joined = string.from(first);
                for (let item in rest) {
                    joined = joined + separator + string.from(item);
                }
                joined
            },
        }
    }

    /// Applies `f` to each element and returns an array of the results.
    //baml:mut_vm
    //baml:may_yield
    function map<U, E>(self, f: (T) -> U throws E) -> U[] throws E {
        $rust_function
    }

    // ─── Predicate scans (callback-driven) ────────────────────────────────────
    //
    // These methods all take a callback and either short-circuit (`some`,
    // `every`, `find*`) or accumulate over the whole array (`reduce`,
    // `flat_map`). On callback error, the error propagates immediately.

    /// Returns `true` if `predicate` returns true for any element. Short-circuits
    /// on the first true: subsequent elements are not visited. Empty array
    /// returns `false`.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3].some((x: int) -> bool { x > 2 })   // true
    /// [1, 2, 3].some((x: int) -> bool { x > 9 })   // false
    /// [].some((x: int) -> bool { true })           // false
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function some<E>(self, predicate: (T) -> bool throws E) -> bool throws E {
        $rust_function
    }

    /// Returns `true` if `predicate` returns true for *every* element.
    /// Short-circuits on the first false. Empty array returns `true` (the
    /// universal-quantifier convention).
    ///
    /// # Examples
    /// ```
    /// [2, 4, 6].every((x: int) -> bool { x % 2 == 0 })   // true
    /// [2, 4, 5].every((x: int) -> bool { x % 2 == 0 })   // false
    /// [].every((x: int) -> bool { false })               // true
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function every<E>(self, predicate: (T) -> bool throws E) -> bool throws E {
        $rust_function
    }

    /// Returns the first element for which `predicate` returns true, or `null`
    /// if no element matches. Short-circuits on the first match.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3, 4].find((x: int) -> bool { x > 2 })   // 3
    /// [1, 2, 3].find((x: int) -> bool { x > 9 })      // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find<E>(self, predicate: (T) -> bool throws E) -> T? throws E {
        $rust_function
    }

    /// Returns the index of the first element for which `predicate` returns
    /// true, or `null` if no element matches. Short-circuits on the first match.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3, 4].find_index((x: int) -> bool { x > 2 })   // 2
    /// [1, 2].find_index((x: int) -> bool { x > 9 })         // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_index<E>(self, predicate: (T) -> bool throws E) -> int? throws E {
        $rust_function
    }

    /// Like `find`, but iterates from the end of the array. Returns the *last*
    /// element for which `predicate` returns true, or `null`. Short-circuits.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3, 4].find_last((x: int) -> bool { x > 1 })   // 4
    /// [1, 2, 3].find_last((x: int) -> bool { x > 9 })      // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_last<E>(self, predicate: (T) -> bool throws E) -> T? throws E {
        $rust_function
    }

    /// Like `find_index`, but iterates from the end of the array. Returns the
    /// index of the *last* matching element, or `null`. Short-circuits.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 1, 2].find_last_index((x: int) -> bool { x == 1 })   // 2
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_last_index<E>(self, predicate: (T) -> bool throws E) -> int? throws E {
        $rust_function
    }

    /// Returns `true` if the array contains an element equal to `item`.
    /// Equality matches BAML's `==` operator: by-value for primitives, by-content
    /// for strings / uint8arrays / variants, by-reference for other objects.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3].includes(2)              // true
    /// [1, 2, 3].includes(99)             // false
    /// ["a", "b"].includes("b")           // true
    /// ```
    //baml:vm
    function includes(self, item: T) -> bool throws never {
        $rust_function
    }

    /// Returns the index of the first element equal to `item`, or `null` if
    /// not found. Equality semantics match `includes`.
    ///
    /// # Examples
    /// ```
    /// [10, 20, 30].index_of(20)   // 1
    /// [10, 20].index_of(99)       // null
    /// ```
    //baml:vm
    function index_of(self, item: T) -> int? throws never {
        $rust_function
    }

    /// Returns the index of the last element equal to `item`, or `null` if
    /// not found. Equality semantics match `includes`.
    ///
    /// # Examples
    /// ```
    /// [10, 20, 10].last_index_of(10)   // 2
    /// [10, 20].last_index_of(99)       // null
    /// ```
    //baml:vm
    function last_index_of(self, item: T) -> int? throws never {
        $rust_function
    }

    /// Folds the array into a single value by repeatedly applying `reducer`.
    ///
    /// Starts from `initial`. For each element `t` (left-to-right) the
    /// accumulator is updated to `reducer(acc, t)`. Returns the final
    /// accumulator (or `initial` if the array is empty).
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0)   // 10
    /// [].reduce((acc: int, x: int) -> int { acc + x }, 42)            // 42
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function reduce<A, E>(self, reducer: (A, T) -> A throws E, initial: A) -> A throws E {
        $rust_function
    }

    /// Maps each element to an array via `f`, then flattens one level.
    ///
    /// Equivalent in spirit to `arr.map(f).flat()` but allocates only the final
    /// flattened array.
    ///
    /// # Examples
    /// ```
    /// [1, 2, 3].flat_map((x: int) -> int[] { [x, x * 10] })
    ///   // [1, 10, 2, 20, 3, 30]
    /// [1, 2].flat_map((x: int) -> int[] { [] })   // []
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function flat_map<U, E>(self, f: (T) -> U[] throws E) -> U[] throws E {
        $rust_function
    }

    /// Removes all elements from the array, leaving it with `length() == 0`.
    /// Mutates `self` in place.
    ///
    /// # Examples
    /// ```
    /// let xs = [1, 2, 3];
    /// xs.clear();
    /// xs.length()  // 0
    /// ```
    //baml:mut_self
    function clear(self) -> null throws never {
        $rust_function
    }

    /// Removes and returns the first element of the array. Returns `null` if
    /// the array is empty. Mutates `self` in place.
    ///
    /// `O(n)` because subsequent elements shift forward — for `O(1)` removal
    /// from the *back* use `pop`.
    ///
    /// # Examples
    /// ```
    /// let xs = [1, 2, 3];
    /// xs.shift()      // 1
    /// xs              // [2, 3]
    /// [].shift()      // null
    /// ```
    //baml:mut_self
    function shift(self) -> T? throws never {
        $rust_function
    }

    /// Inserts `item` at the front of the array, shifting all existing
    /// elements one position to the right. Returns the new length.
    /// Mutates `self` in place.
    ///
    /// `O(n)` because all subsequent elements move — for `O(1)` insertion at
    /// the *back* use `push`.
    ///
    /// # Examples
    /// ```
    /// let xs = [2, 3];
    /// xs.unshift(1)   // 3   (new length)
    /// xs              // [1, 2, 3]
    /// ```
    //baml:mut_self
    function unshift(self, item: T) -> int throws never {
        $rust_function
    }

    /// Inserts `item` at index `idx`, shifting existing elements at
    /// `[idx, length())` one position to the right. Returns the new length.
    /// Mutates `self` in place.
    ///
    /// `idx` may equal `length()`, in which case the call is equivalent to
    /// `push(item)`. Negative indices count from the end, so `-1` inserts before
    /// the last element. Throws `InvalidArgument` if `idx` is outside the range
    /// `[-length(), length()]`.
    ///
    /// # Examples
    /// ```
    /// let xs = [1, 2, 4];
    /// xs.insert(3, 2)        // 4   inserts 3 at index 2; xs is now [1, 2, 3, 4]
    /// xs.insert(9, -1)       // 5   inserts before the last; xs is now [1, 2, 3, 9, 4]
    /// xs.insert(0, 100)      // throws — idx beyond length
    /// ```
    //baml:mut_self
    function insert(self, item: T, idx: int) -> int throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Replaces the elements at `[start, start + count)` with the contents of
    /// `replace`. Mutates `self` in place. The replacement may be longer or
    /// shorter than `count`; the array grows or shrinks accordingly.
    ///
    /// A negative `start` counts from the end. Throws `InvalidArgument` if `start`
    /// is outside the range `[-length(), length()]`. If `start + count` would
    /// exceed `length()`, `count` is clamped to `length() - start`. `count` must
    /// be non-negative.
    ///
    /// # Examples
    /// ```
    /// let xs = [0, 1, 2, 3, 4];
    /// xs.splice(1, 2, [9, 9, 9])   // xs is now [0, 9, 9, 9, 3, 4]
    /// xs.splice(0, 0, [-1])        // pure insertion: [-1, 0, 9, 9, 9, 3, 4]
    /// xs.splice(-1, 1, [])         // removes the last element: [-1, 0, 9, 9, 9, 3]
    /// xs.splice(99, 0, [])         // throws — start out of range
    /// ```
    //baml:mut_self
    function splice(
        self,
        start: int,
        count: int,
        replace: T[],
    ) -> null throws root.errors.InvalidArgument {
        $rust_function
    }
}

/// An insertion-ordered collection of key-value pairs.
///
/// Iteration order is the order in which keys were first inserted: `keys()`
/// and `values()` follow it, replacing an existing key with `set` keeps the
/// key's original position, and `delete` preserves the order of the
/// remaining entries.
class Map<K, V> {
    /// Returns the number of entries in the map.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns `true` if the map contains an entry for `key`.
    //baml:vm
    function has(self, key: K) -> bool throws never {
        $rust_function
    }

    /// Returns an array of all keys in the map, in insertion order.
    //baml:mut_vm
    function keys(self) -> K[] throws never {
        $rust_function
    }

    /// Returns an array of all values in the map. Order matches `keys()`.
    function values(self) -> V[] throws never {
        $rust_function
    }

    /// Inserts or replaces the entry for `key`. Returns the previously
    /// associated value, or `null` if `key` was not already present.
    /// Replacing an existing key keeps its original position in iteration
    /// order; a new key is appended at the end.
    ///
    /// # Examples
    /// ```
    /// let m: map<string, int> = {};
    /// m.set("a", 1)         // null  (was absent)
    /// m.set("a", 2)         // 1     (returns the old value)
    /// m.get("a")            // 2
    /// ```
    //baml:mut_self
    function set(self, key: K, value: V) -> V? throws never {
        $rust_function
    }

    /// Returns the value associated with `key`, or `null` if the key is not
    /// present.
    ///
    /// # Examples
    /// ```
    /// let m: map<string, int> = {"a": 1};
    /// m.get("a")     // 1
    /// m.get("b")     // null
    /// ```
    //baml:vm
    function get(self, key: K) -> V? throws never {
        $rust_function
    }

    /// Removes and returns the entry for `key`. Returns `null` if the key was
    /// not present. Preserves the insertion order of the remaining entries.
    ///
    /// # Examples
    /// ```
    /// let m: map<string, int> = {"a": 1, "b": 2};
    /// m.delete("a")     // 1
    /// m.delete("a")     // null  (already gone)
    /// m.length()        // 1
    /// ```
    //baml:mut_self
    function delete(self, key: K) -> V? throws never {
        $rust_function
    }

    /// Returns the existing value for `key`, or inserts `default` and returns
    /// it if the key was absent. Mutates `self` only when the key was absent.
    ///
    /// `default` is always evaluated by the caller. To compute a default
    /// lazily (only when needed), see `get_or_insert_computed` (deferred).
    ///
    /// # Examples
    /// ```
    /// let m: map<string, int> = {};
    /// m.get_or_insert("k", 10)   // 10  (inserted)
    /// m.get_or_insert("k", 99)   // 10  (existing — `99` ignored)
    /// ```
    //baml:mut_self
    function get_or_insert(self, key: K, default: V) -> V throws never {
        $rust_function
    }

    /// Removes all entries from the map, leaving it with `length() == 0`.
    /// Mutates `self` in place.
    ///
    /// # Examples
    /// ```
    /// let m: map<string, int> = {"a": 1, "b": 2};
    /// m.clear();
    /// m.length()    // 0
    /// ```
    //baml:mut_self
    function clear(self) -> null throws never {
        $rust_function
    }
}

/// Summation for numeric arrays.
///
/// Implemented for `int[]` (returning `int`) and `float[]` (returning `float`).
/// `Sum` is the element's own numeric type, so `xs.sum()` stays in the array's
/// number domain with no implicit widening — `int` and `float` are distinct
/// types, and there is deliberately no `int[] -> float` sum. To sum integer
/// data as floats, map first: `ints.map((x: int) -> float { x * 1.0 }).sum()`.
///
/// The method form `xs.sum()` is the only public surface; it delegates to the
/// private native `_sum_int` / `_sum_float` helpers declared below.
interface Summable {
  type Sum

  function sum(self) -> Self.Sum throws never
}

/// Sums an `int[]` and returns an `int`.
///
/// Adds every element left-to-right starting from `0`, so an empty array sums
/// to `0`. Like repeated `+`, a running total that leaves the `int` range
/// raises `baml.panics.IntegerOverflow`.
///
/// # Examples
/// ```
/// [1, 2, 3].sum()   // 6
/// [].sum()          // 0
/// [-5, 5].sum()     // 0
/// ```
implements Summable for int[] {
  type Sum = int

  function sum(self) -> int throws never {
    root._sum_int(self)
  }
}

/// Sums a `float[]` and returns a `float`.
///
/// Adds every element left-to-right starting from `0.0`, so an empty array sums
/// to `0.0`. Never throws.
///
/// # Examples
/// ```
/// [1.0, 2.0, 3.0].sum()   // 6.0
/// [].sum()                // 0.0
/// ```
implements Summable for float[] {
  type Sum = float

  function sum(self) -> float throws never {
    root._sum_float(self)
  }
}

/// Statistical reductions for `float[]`.
///
/// `mean` and `median` are defined only for `float[]` (both return `float`),
/// so — unlike `Summable`, which also covers `int[]` — this interface has a
/// single implementor. It exists so the reductions attach as methods
/// (`xs.mean()`, `xs.median()`) on the structural `float[]` type; the backing
/// natives are the private `_mean_float` / `_median_float` helpers below.
interface FloatStats {
  function mean(self) -> float throws root.errors.InvalidArgument
  function median(self) -> float throws root.errors.InvalidArgument
}

/// Statistical reductions over a `float[]` — see `FloatStats`.
implements FloatStats for float[] {
  /// Returns the arithmetic mean (average) of `self`.
  ///
  /// Computed as `self.sum() / self.length()`. Throws
  /// `root.errors.InvalidArgument` when `self` is empty, because the mean of
  /// no elements is undefined.
  ///
  /// # Examples
  /// ```
  /// [1.0, 2.0, 3.0, 4.0].mean()   // 2.5
  /// [5.0].mean()                  // 5.0
  /// [].mean()                     // throws InvalidArgument
  /// ```
  function mean(self) -> float throws root.errors.InvalidArgument {
    root._mean_float(self)
  }

  /// Returns the median (the middle value) of `self`.
  ///
  /// The elements are copied and sorted ascending (IEEE 754 `totalOrder`,
  /// matching `float[].sort()`) before the middle is taken, so the caller's
  /// array is left untouched. With an odd element count the single middle
  /// element is returned; with an even count the arithmetic mean of the two
  /// middle elements is returned. Throws `root.errors.InvalidArgument` when
  /// `self` is empty.
  ///
  /// # Examples
  /// ```
  /// [3.0, 1.0, 2.0].median()        // 2.0  (odd count)
  /// [1.0, 2.0, 3.0, 4.0].median()   // 2.5  (mean of the two middles)
  /// [].median()                     // throws InvalidArgument
  /// ```
  function median(self) -> float throws root.errors.InvalidArgument {
    root._median_float(self)
  }
}

// ─── Native backings for the numeric-array reductions ─────────────────────────
// Private helpers (leading `_`) that were formerly the public `baml.math.*`
// aggregation builtins. The method forms above and `Summable.sum` are now the
// only public surface; these are the thin native shims they delegate to.

/// Native backing for `int[].sum()`. Adds every element left-to-right starting
/// from `0`; a running total that leaves the `int` range raises
/// `baml.panics.IntegerOverflow`. The empty array sums to `0`.
//baml:fallible
function _sum_int(values: int[]) -> int throws never {
    $rust_function
}

/// Native backing for `float[].sum()`. Adds every element left-to-right
/// starting from `0.0`; the empty array sums to `0.0`. Never throws.
//baml:vm
function _sum_float(values: float[]) -> float throws never {
    $rust_function
}

/// Native backing for `float[].mean()`. Throws `root.errors.InvalidArgument`
/// when `values` is empty.
//baml:vm
function _mean_float(values: float[]) -> float throws root.errors.InvalidArgument {
    $rust_function
}

/// Native backing for `float[].median()`. Throws `root.errors.InvalidArgument`
/// when `values` is empty.
//baml:vm
function _median_float(values: float[]) -> float throws root.errors.InvalidArgument {
    $rust_function
}
