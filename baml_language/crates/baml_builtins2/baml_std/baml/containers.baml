/// An ordered, growable sequence of `T`.
///
/// `Array<T>` and the `T[]` shorthand are the same type. Values come from an
/// array literal (`[1, 2, 3]`) or from the `Array.filled` / `Array.generate`
/// constructors below.
///
/// Arrays are heap objects with reference semantics: binding one to a second
/// name, passing it to a function, or storing it inside another container
/// shares the one array, so every mutating method (`push`, `pop`, `sort_by`,
/// `clear`, …) is visible through every reference to it. The methods that
/// return a `T[]` instead — `concat`, `slice`, `reverse`, `map`, `filter`, … —
/// build a fresh array, but they copy only one level deep: the elements are the
/// same values, so a nested array, map, or class instance stays shared.
///
/// `arr[i]` reads and `arr[i] = v` writes through the array. Both accept a
/// negative index, counting back from the end (`-1` is the last element), and
/// both raise `baml.panics.IndexOutOfBounds` for an index that still lands
/// outside the array; `at` is the non-panicking read. Where JavaScript answers
/// a failed lookup or search with `undefined` or `-1`, the methods here answer
/// `null`.
///
/// Every method that takes a callback walks a snapshot of the array taken
/// before the first call, so a callback may mutate `self` without changing
/// which elements are visited. An error thrown by a callback propagates
/// immediately, stopping the walk and discarding any partial result.
class Array<T> {
    /// Returns an array of `length` elements, every one of them `value`.
    ///
    /// The idiomatic way to allocate a runtime-sized, pre-initialized buffer —
    /// Fenwick trees, DP tables, adjacency lists — without a `while`/`push`
    /// loop.
    ///
    /// Every slot holds that one `value`, so for a reference type all slots
    /// alias the same object and mutating one slot mutates them all. Use
    /// `Array.generate` for an independent value per slot.
    ///
    /// # Parameters
    /// - `length`: how many elements to create. Zero or negative produces an
    ///   empty array.
    /// - `value`: the value stored in every slot.
    ///
    /// # Examples
    /// ```baml
    /// Array.filled(3, 0)     // [0, 0, 0]
    /// Array.filled(-1, 0)    // []
    ///
    /// let shared: int[] = [];
    /// let rows = Array.filled(3, shared);   // every slot is `shared` itself
    /// rows[0].push(1);                      // rows is now [[1], [1], [1]]
    /// ```
    function filled(length: int, value: T) -> T[] throws never {
        $rust_function
    }

    /// Returns an array of `length` elements, where element `i` is `f(i)`.
    ///
    /// Unlike `Array.filled`, which reuses one shared value, every slot gets an
    /// independent value — the alias-free way to build runtime-sized buffers of
    /// reference types, such as the rows of a grid. This is JavaScript's
    /// [`Array.from({ length }, f)`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/from)
    /// and a Python list comprehension.
    ///
    /// # Parameters
    /// - `length`: how many elements to create. Zero or negative produces an
    ///   empty array and never calls `f`.
    /// - `f`: called once per index `0, 1, …, length - 1`, in order, to produce
    ///   that slot's value.
    ///
    /// # Examples
    /// ```baml
    /// Array.generate(3, (i: int) -> int { i * i })      // [0, 1, 4]
    /// // A 2D grid whose rows can be mutated independently:
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

    /// Returns the number of elements. `O(1)` — the length is stored, not
    /// counted.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns the element at `index`, or `null` if out of bounds.
    ///
    /// Resolves `index` exactly as the subscript does, negative indices
    /// included, but returns `null` where `self[index]` raises
    /// `baml.panics.IndexOutOfBounds`. This is JavaScript's
    /// [`Array.prototype.at`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/at),
    /// with `null` in place of `undefined`.
    ///
    /// When `T` is itself nullable the result is ambiguous: a stored `null` and
    /// an out-of-range index are indistinguishable. Compare `index` against
    /// `length()` when the difference matters.
    ///
    /// # Examples
    /// ```baml
    /// [10, 20, 30].at(-1)   // 30
    /// [10, 20, 30].at(99)   // null
    /// ```
    function at(self, index: int) -> T? throws never {
        $rust_function
    }

    /// Appends `item` to the end of the array and returns the new length.
    /// Amortized `O(1)`.
    //baml:mut_self
    function push(self, item: T) -> int throws never {
        $rust_function
    }

    /// Removes and returns the last element, or `null` if the array is empty.
    /// `O(1)` — the inverse of `push`.
    ///
    /// When `T` is nullable, `null` means either "the array was empty" or "the
    /// last element was `null`". Drive a drain loop off `length()` in that
    /// case.
    //baml:mut_self
    function pop(self) -> T? throws never {
        $rust_function
    }

    /// Removes and returns the element at `index`, or `null` if out of bounds.
    ///
    /// Resolves `index` as `at` does; an out-of-range `index` leaves the array
    /// unchanged. `O(n)`, because later elements shift backward — `pop` is the
    /// `O(1)` removal from the end.
    ///
    /// # Examples
    /// ```baml
    /// let xs = [10, 20, 30];
    /// xs.remove_at(1)   // 20   xs is now [10, 30]
    /// xs.remove_at(-1)  // 30   xs is now [10]
    /// xs.remove_at(99)  // null
    /// ```
    //baml:mut_self
    function remove_at(self, index: int) -> T? throws never {
        $rust_function
    }

    /// Returns the elements of `self` followed by the elements of `other`.
    /// Neither input is mutated.
    function concat(self, other: T[]) -> T[] throws never {
        $rust_function
    }

    /// Calls `fn` once for each element, left to right, discarding its result.
    function for_each<E>(self, fn: (T) -> void throws E) -> void throws E {
        for (let x in self.slice(0, self.length())) {
            fn(x);
        }
    }

    /// Returns the elements matching `predicate`, in order.
    ///
    /// JavaScript's
    /// [`Array.prototype.filter`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/filter).
    /// For a lazy version, use `iter().filter(predicate)`.
    //baml:mut_vm
    //baml:may_yield
    function filter<E>(self, predicate: (T) -> bool throws E) -> T[] throws E {
        $rust_function
    }

    /// Returns the non-null results of `fn` — mapping and filtering in one
    /// pass.
    ///
    /// A `null` result drops that element, which makes a mapped-to `null`
    /// indistinguishable from a drop when `U` is itself nullable. For a lazy
    /// version, use `iter().filter_map(fn)`.
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

    /// Returns the elements in reverse order.
    ///
    /// Unlike JavaScript's `Array.prototype.reverse`, which reverses in place,
    /// this is the copying form — the equivalent is
    /// [`toReversed`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/toReversed).
    ///
    /// # Examples
    ///
    /// ```baml
    /// let arr = [1, 2, 3];
    /// let reversed = arr.reverse();
    /// assert.equal(reversed, [3, 2, 1]);
    /// ```
    function reverse(self) -> T[] throws never {
        $rust_function
    }

    /// Sorts the array in place and returns `self` — the same array object, not
    /// a copy.
    ///
    /// `compare` follows the JavaScript
    /// [`sort`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/sort)
    /// convention: negative sorts the first argument before the second, zero
    /// keeps their relative order, positive sorts it after. The sort is stable,
    /// so elements the comparator calls equal stay in their original order.
    ///
    /// Comparator calls run sequentially in a deterministic merge order, but
    /// that order is an implementation detail; a comparator must not depend on
    /// a particular comparison sequence.
    ///
    /// The write-back happens only once the whole sort completes. A comparator
    /// that throws therefore leaves `self` in its pre-sort order, and one that
    /// mutates `self` has those mutations discarded.
    //baml:mut_vm
    //baml:may_yield
    //baml:mut_self
    function sort_by<E>(self, compare: (T, T) -> int throws E) -> T[] throws E {
        $rust_function
    }

    /// Sorts the array in place by a key, then returns `self` — the same array
    /// object, not a copy.
    ///
    /// Each key is computed exactly once, left to right, and the elements are
    /// then ordered by their keys' natural order (`Comparable.compare`). That
    /// compute-once guarantee is what makes this cheaper than the equivalent
    /// `sort_by` when the key is expensive to derive. The sort is stable, so
    /// elements with equal keys keep their original order.
    ///
    /// Like `sort_by`, the write-back happens only on success: a `key` or
    /// comparison that throws leaves `self` in its pre-sort order, and a `key`
    /// that mutates `self` has those mutations discarded.
    ///
    /// The key type must be `Comparable`, and `U.CompareError` — the key's own
    /// comparison error — propagates alongside `E`. For `int`, `bigint`,
    /// `string` and `float` keys that is `never`, leaving only `E`.
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

    /// Returns the elements from `start` (inclusive) to `end` (exclusive).
    ///
    /// Negative bounds count from the end. Unlike `at` and the subscript, an
    /// out-of-range bound is clamped into `[0, length()]` rather than reported,
    /// and an `end` at or before `start` yields an empty array — so `slice`
    /// never fails. This is JavaScript's
    /// [`Array.prototype.slice`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/slice),
    /// with both bounds required. `slice(0, length())` is the idiomatic shallow
    /// copy.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3, 4].slice(1, 3)   // [2, 3]
    /// [1, 2, 3].slice(-2, 3)     // [2, 3]
    /// [1, 2, 3].slice(2, 1)      // []
    /// ```
    function slice(self, start: int, end: int) -> T[] throws never {
        $rust_function
    }

    /// Joins the elements into one string, separated by `separator`.
    ///
    /// Each element is rendered with `string.from`: a runtime type implementing
    /// `baml.ToString` uses its `to_string` override, and everything else falls
    /// back to the default structural rendering. `separator` appears exactly
    /// `length() - 1` times, so an empty array joins to `""` and a one-element
    /// array to just that element's rendering.
    ///
    /// # Examples
    ///
    /// ```baml
    /// [1, 2, 3].join(",")  // "1,2,3"
    /// [true, false, true].join("||")  // "true||false||true"
    /// ```
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

    /// Applies `f` to each element and returns the results, in order.
    ///
    /// JavaScript's
    /// [`Array.prototype.map`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/map).
    /// For a lazy version, use `iter().map(f)`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// [1, 2, 3].map((x: int) -> int { x * 2 })                   // [2, 4, 6]
    /// ["a", "bb", "ccc"].map((x: string) -> int { x.length() })  // [1, 2, 3]
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function map<U, E>(self, f: (T) -> U throws E) -> U[] throws E {
        $rust_function
    }

    // ─── Predicate scans (callback-driven) ────────────────────────────────────

    /// Returns `true` if `predicate` returns true for any element.
    ///
    /// Scans left to right and short-circuits on the first true, so later
    /// elements are never passed to `predicate`. An empty array returns
    /// `false`. This is JavaScript's
    /// [`some`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/some)
    /// and Python's `any`.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3].some((x: int) -> bool { x > 2 })   // true
    /// [].some((x: int) -> bool { true })           // false
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function some<E>(self, predicate: (T) -> bool throws E) -> bool throws E {
        $rust_function
    }

    /// Returns `true` if `predicate` returns true for *every* element.
    ///
    /// Scans left to right and short-circuits on the first false. An empty
    /// array returns `true`. This is JavaScript's
    /// [`every`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/every)
    /// and Python's `all`.
    ///
    /// # Examples
    /// ```baml
    /// [2, 4, 6].every((x: int) -> bool { x % 2 == 0 })   // true
    /// [].every((x: int) -> bool { false })               // true
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function every<E>(self, predicate: (T) -> bool throws E) -> bool throws E {
        $rust_function
    }

    /// Returns the first element matching `predicate`, or `null` if none does.
    ///
    /// Scans left to right and short-circuits on the first match. When `T` is
    /// nullable the result is ambiguous — a matched `null` element and "no
    /// match" look the same; use `find_index` there.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3, 4].find((x: int) -> bool { x > 2 })   // 3
    /// [1, 2, 3].find((x: int) -> bool { x > 9 })      // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find<E>(self, predicate: (T) -> bool throws E) -> T? throws E {
        $rust_function
    }

    /// Returns the index of the first element matching `predicate`, or `null`
    /// if none does.
    ///
    /// Scans left to right and short-circuits on the first match. JavaScript's
    /// `findIndex`, but a miss is `null` rather than `-1`.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3, 4].find_index((x: int) -> bool { x > 2 })   // 2
    /// [1, 2].find_index((x: int) -> bool { x > 9 })         // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_index<E>(self, predicate: (T) -> bool throws E) -> int? throws E {
        $rust_function
    }

    /// Returns the *last* element matching `predicate`, or `null` if none does.
    ///
    /// Like `find`, but scans right to left. A nullable `T` makes the result
    /// ambiguous the same way; use `find_last_index` there.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3, 4].find_last((x: int) -> bool { x > 1 })   // 4
    /// [1, 2, 3].find_last((x: int) -> bool { x > 9 })      // null
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_last<E>(self, predicate: (T) -> bool throws E) -> T? throws E {
        $rust_function
    }

    /// Returns the index of the *last* element matching `predicate`, or `null`
    /// if none does.
    ///
    /// Like `find_index`, but scans right to left. The index is still counted
    /// from the front.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 1, 2].find_last_index((x: int) -> bool { x == 1 })   // 2
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function find_last_index<E>(self, predicate: (T) -> bool throws E) -> int? throws E {
        $rust_function
    }

    /// Returns `true` if the array contains an element equal to `item`. Scans
    /// left to right, stopping at the first match; `O(n)`.
    ///
    /// Equality here is a fixed built-in test, *not* the `==` operator. `int`,
    /// `bool` and `null` compare by value; `float` by IEEE equality, so a `NaN`
    /// element never matches and `0.0` matches `-0.0`; `string` and
    /// `uint8array` by content; enum variants by variant identity. Everything
    /// else — `bigint`, arrays, maps, class instances, `type` values — compares
    /// by *reference*, so a structurally identical but separately constructed
    /// value is not found. For `==`'s deep, override-aware comparison, spell it
    /// out: `self.some((x: T) -> bool { x == item })`.
    ///
    /// JavaScript's
    /// [`includes`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/includes)
    /// matches `NaN` against itself; this does not.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3].includes(2)     // true
    /// ["a", "b"].includes("b")  // true
    /// [[1]].includes([1])       // false — by reference, though [1] == [1]
    /// ```
    //baml:vm
    function includes(self, item: T) -> bool throws never {
        $rust_function
    }

    /// Returns the index of the first element equal to `item`, or `null` if
    /// there is none. `O(n)`.
    ///
    /// Equality is the built-in test described on `includes` — notably *not*
    /// the `==` operator for `bigint` or for container and class-instance
    /// elements. JavaScript's `indexOf`, but a miss is `null` rather than `-1`.
    ///
    /// # Examples
    /// ```baml
    /// [10, 20, 30].index_of(20)   // 1
    /// [10, 20].index_of(99)       // null
    /// ```
    //baml:vm
    function index_of(self, item: T) -> int? throws never {
        $rust_function
    }

    /// Returns the index of the last element equal to `item`, or `null` if
    /// there is none.
    ///
    /// Equality is the built-in test described on `includes`. Scans right to
    /// left, so it stops early on a match near the end but is `O(n)` in the
    /// worst case.
    ///
    /// # Examples
    /// ```baml
    /// [10, 20, 10].last_index_of(10)   // 2
    /// [10, 20].last_index_of(99)       // null
    /// ```
    //baml:vm
    function last_index_of(self, item: T) -> int? throws never {
        $rust_function
    }

    /// Folds the array into a single value, starting from `initial`.
    ///
    /// Each element `t`, left to right, updates the accumulator to
    /// `reducer(acc, t)`; the final accumulator is returned. An empty array
    /// returns `initial` unchanged. The argument order is
    /// `(accumulator, element)`, as in JavaScript's
    /// [`reduce`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/reduce)
    /// — but `initial` is mandatory here, so reducing an empty array is never
    /// an error.
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3, 4].reduce((acc: int, x: int) -> int { acc + x }, 0)   // 10
    /// [].reduce((acc: int, x: int) -> int { acc + x }, 42)            // 42
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function reduce<A, E>(self, reducer: (A, T) -> A throws E, initial: A) -> A throws E {
        $rust_function
    }

    /// Maps each element to an array via `f`, then concatenates the results —
    /// one level of flattening, never more.
    ///
    /// Only the flattened result is allocated; the intermediate array of arrays
    /// never exists. This is JavaScript's
    /// [`flatMap`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/flatMap).
    ///
    /// # Examples
    /// ```baml
    /// [1, 2, 3].flat_map((x: int) -> int[] { [x, x * 10] })
    ///   // [1, 10, 2, 20, 3, 30]
    /// [1, 2].flat_map((x: int) -> int[] { [] })   // []
    /// ```
    //baml:mut_vm
    //baml:may_yield
    function flat_map<U, E>(self, f: (T) -> U[] throws E) -> U[] throws E {
        $rust_function
    }

    /// Removes every element, leaving `length() == 0`.
    ///
    /// The array object is emptied, not replaced, so every other reference to
    /// it observes the empty array too. Rebind to a fresh `[]` to leave the
    /// original alone.
    //baml:mut_self
    function clear(self) -> null throws never {
        $rust_function
    }

    /// Removes and returns the first element, or `null` if the array is empty.
    ///
    /// `O(n)`, because every later element shifts forward — `pop` is the `O(1)`
    /// removal from the *back*. As with `pop`, a nullable `T` leaves `null`
    /// ambiguous between "empty" and "the first element was `null`".
    ///
    /// # Examples
    /// ```baml
    /// let xs = [1, 2, 3];
    /// xs.shift()      // 1
    /// xs              // [2, 3]
    /// [].shift()      // null
    /// ```
    //baml:mut_self
    function shift(self) -> T? throws never {
        $rust_function
    }

    /// Inserts `item` at the front of the array and returns the new length.
    ///
    /// `O(n)`, because every existing element shifts one position right —
    /// `push` is the `O(1)` insertion at the *back*.
    ///
    /// # Examples
    /// ```baml
    /// let xs = [2, 3];
    /// xs.unshift(1)   // 3   (new length)
    /// xs              // [1, 2, 3]
    /// ```
    //baml:mut_self
    function unshift(self, item: T) -> int throws never {
        $rust_function
    }

    /// Inserts `item` at index `idx` and returns the new length. `O(n)`, since
    /// the elements at `[idx, length())` shift one position right.
    ///
    /// `idx` is an *insertion* position, so unlike `at` it accepts the whole
    /// closed range `[0, length()]` — inserting at `length()` is `push(item)`.
    /// Negative indices count from the end, so `-1` inserts *before* the last
    /// element and `-length()` at the front; the append position has no
    /// negative spelling.
    ///
    /// # Throws
    /// - `baml.errors.InvalidArgument` if `idx` falls outside
    ///   `[-length(), length()]`; `self` is left unchanged.
    ///
    /// # Examples
    /// ```baml
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
    /// `replace`.
    ///
    /// The replacement may be longer or shorter than `count`, so the array
    /// grows or shrinks; `count == 0` makes this a pure insertion at `start`.
    /// `start` is an insertion position, resolved exactly as `insert`'s `idx`:
    /// negatives count from the end, and the closed range
    /// `[-length(), length()]` is valid. A `start + count` past the end clamps
    /// `count` to `length() - start` rather than reporting it.
    ///
    /// Unlike JavaScript's
    /// [`Array.prototype.splice`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/splice),
    /// the removed elements are *not* returned — take a `slice` first if you
    /// need them. The elements of `replace` are moved in as-is, so a reference
    /// type stays shared between `replace` and `self`.
    ///
    /// # Throws
    /// - `baml.errors.InvalidArgument` if `start` falls outside
    ///   `[-length(), length()]`, or if `count` is negative; `self` is left
    ///   unchanged in both cases.
    ///
    /// # Examples
    /// ```baml
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
/// `Map<K, V>` is spelled `map<K, V>` in type position. `K` must be `string` —
/// the compiler rejects any other key type — so the type parameter exists for
/// uniformity rather than choice; convert other keys with `.to_string()` first.
///
/// Iteration order is the order in which keys were *first* inserted: `keys()`
/// and `values()` follow it, `set` on an existing key keeps that key's original
/// position, and `delete` preserves the order of the entries that remain. This
/// is how JavaScript's
/// [`Map`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map)
/// and Python's `dict` behave — not the arbitrary order of a hash map.
///
/// Like arrays, maps are heap objects with reference semantics: a map passed to
/// a function or stored in another container is shared, and every mutating
/// method (`set`, `delete`, `clear`, …) is visible through every reference.
///
/// `m[key]` reads and `m[key] = v` writes through the map. Reading a key that
/// is not present raises `baml.panics.MapKeyNotFound`; writing one inserts it
/// at the end. `get` is the non-panicking read.
///
/// When `V` is nullable, a `null` returned by `get`, `set` or `delete` is
/// ambiguous between "no such key" and "the key's value is `null`". `has` is
/// the way to tell the two apart.
class Map<K, V> {
    /// Returns the number of entries. `O(1)`.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns `true` if the map contains an entry for `key`. `O(1)`.
    ///
    /// Prefer this to `get(key) != null`: a key mapped to `null` is present,
    /// but `get` cannot say so.
    //baml:vm
    function has(self, key: K) -> bool throws never {
        $rust_function
    }

    /// Returns the keys, in insertion order.
    ///
    /// The array is a snapshot, not a live view: later `set` and `delete` calls
    /// do not change it, and mutating it does not change the map. Iterating it
    /// while mutating the map is therefore safe.
    //baml:mut_vm
    function keys(self) -> K[] throws never {
        $rust_function
    }

    /// Returns the values, in the same order as `keys()` — so `keys()[i]` maps
    /// to `values()[i]`.
    ///
    /// The array is a snapshot, but the values are not copied: a reference type
    /// is shared with the map, so mutating a value obtained here also mutates
    /// the one the map holds.
    function values(self) -> V[] throws never {
        $rust_function
    }

    /// Inserts or replaces the entry for `key` and returns the previous value,
    /// or `null` if `key` was absent. `O(1)`.
    ///
    /// Replacing an existing key keeps its original position in iteration
    /// order; a new key is appended at the end. `self[key] = value` is the same
    /// operation, minus the previous value.
    ///
    /// # Examples
    /// ```baml
    /// let m: map<string, int> = {};
    /// m.set("a", 1)         // null  (was absent)
    /// m.set("a", 2)         // 1     (returns the old value)
    /// ```
    //baml:mut_self
    function set(self, key: K, value: V) -> V? throws never {
        $rust_function
    }

    /// Returns the value for `key`, or `null` if the key is absent. `O(1)`.
    ///
    /// The non-panicking counterpart of `self[key]`, which raises
    /// `baml.panics.MapKeyNotFound` where `get` returns `null`.
    ///
    /// # Examples
    /// ```baml
    /// let m: map<string, int> = {"a": 1};
    /// m.get("a")     // 1
    /// m.get("b")     // null
    /// ```
    //baml:vm
    function get(self, key: K) -> V? throws never {
        $rust_function
    }

    /// Removes the entry for `key` and returns its value, or `null` if the key
    /// was absent.
    ///
    /// The entries that remain keep their insertion order, which costs `O(n)`:
    /// everything after the removed entry shifts down. An order-scrambling
    /// `O(1)` removal is deliberately not offered — iteration order is part of
    /// this type's contract.
    ///
    /// # Examples
    /// ```baml
    /// let m: map<string, int> = {"a": 1, "b": 2};
    /// m.delete("a")     // 1
    /// m.delete("a")     // null  (already gone)
    /// ```
    //baml:mut_self
    function delete(self, key: K) -> V? throws never {
        $rust_function
    }

    /// Returns the value for `key`, inserting `default` first if the key was
    /// absent. Python's
    /// [`dict.setdefault`](https://docs.python.org/3/library/stdtypes.html#dict.setdefault).
    ///
    /// The map is touched only on insertion, and the new key is appended at the
    /// end of the iteration order.
    ///
    /// `default` is an ordinary argument, so it is evaluated at the call site
    /// whether or not it ends up being inserted. Guard an expensive or
    /// side-effecting default behind `has` yourself.
    ///
    /// # Examples
    /// ```baml
    /// let m: map<string, int> = {};
    /// m.get_or_insert("k", 10)   // 10  (inserted)
    /// m.get_or_insert("k", 99)   // 10  (existing — `99` ignored)
    /// ```
    //baml:mut_self
    function get_or_insert(self, key: K, default: V) -> V throws never {
        $rust_function
    }

    /// Removes every entry, leaving `length() == 0`.
    ///
    /// The map object is emptied, not replaced, so every other reference to it
    /// observes the empty map too. Rebind to a fresh `{}` to leave the original
    /// alone.
    //baml:mut_self
    function clear(self) -> null throws never {
        $rust_function
    }
}

/// Summation for numeric arrays.
///
/// Implemented for `int[]`, summing to `int`, and `float[]`, summing to
/// `float`. `Sum` is the element's own numeric type, so `xs.sum()` stays in the
/// array's number domain with no implicit widening: `int` and `float` are
/// distinct types, and there is deliberately no `int[] -> float` sum. To sum
/// integer data as floats, map first —
/// `ints.map((x: int) -> float { x * 1.0 }).sum()`.
interface Summable {
    /// The result of summing `self`.
    type Sum

    /// Adds every element left to right, starting from the additive identity,
    /// so an empty collection sums to that identity.
    ///
    /// `throws never` does not make summation total: an implementation over a
    /// bounded integer type overflows the same way repeated `+` does, and
    /// overflow is a panic rather than an error. See each implementation for
    /// its range behaviour.
    function sum(self) -> Self.Sum throws never
}

implements Summable for int[] {
  type Sum = int

  /// Adds every element left to right starting from `0`, so an empty array
  /// sums to `0`.
  ///
  /// The running total is range-checked at every step, exactly as repeated `+`
  /// is, so a partial sum that leaves the `int` range panics even when the
  /// final total would have landed back inside it. Fold with a `bigint`
  /// accumulator, or map to `float`, when the intermediate values can grow past
  /// `int`.
  ///
  /// # Panics
  /// - `baml.panics.IntegerOverflow` when a running total leaves the `int`
  ///   range.
  ///
  /// # Examples
  /// ```baml
  /// [1, 2, 3].sum()   // 6
  /// [].sum()          // 0
  /// ```
  function sum(self) -> int throws never {
    root._sum_int(self)
  }
}

implements Summable for float[] {
  type Sum = float

  /// Adds every element left to right starting from `0.0`, so an empty array
  /// sums to `0.0`.
  ///
  /// Plain sequential IEEE 754 addition with no compensation: the result
  /// depends on the element order, and a `NaN` or infinity anywhere in the
  /// array propagates to the total. Unlike `int[].sum()` there is no range to
  /// leave — a total too large to represent becomes an infinity rather than
  /// panicking.
  ///
  /// # Examples
  /// ```baml
  /// [1.0, 2.0, 3.0].sum()   // 6.0
  /// [].sum()                // 0.0
  /// ```
  function sum(self) -> float throws never {
    root._sum_float(self)
  }
}

/// Statistical reductions for `float[]`.
///
/// Unlike `Summable`, which also covers `int[]`, this interface has a single
/// implementor: `mean` and `median` are defined only for `float[]`. It exists
/// so the reductions attach as methods — `xs.mean()`, `xs.median()` — to the
/// structural `float[]` type. Both reject the empty array rather than inventing
/// a value for it.
interface FloatStats {
  /// The arithmetic mean (average) of `self`.
  ///
  /// # Throws
  /// - `baml.errors.InvalidArgument` when `self` is empty — the mean of no
  ///   elements is undefined.
  function mean(self) -> float throws root.errors.InvalidArgument

  /// The median of `self`: the middle element once the values are ordered, or
  /// the mean of the two middle elements when the count is even.
  ///
  /// # Throws
  /// - `baml.errors.InvalidArgument` when `self` is empty.
  function median(self) -> float throws root.errors.InvalidArgument
}

implements FloatStats for float[] {
    /// Returns the arithmetic mean (average) of `self`.
    ///
    /// Computed as `self.sum() / self.length()` — one sequential IEEE 754 pass
    /// then a single divide — so it inherits `float[].sum()`'s order
    /// sensitivity and propagates any `NaN` or infinity in the input.
    ///
    /// # Throws
    /// - `baml.errors.InvalidArgument` when `self` is empty, because the mean
    ///   of no elements is undefined. It is an error rather than a `NaN` so the
    ///   caller has to decide what an empty sample means.
    ///
    /// # Examples
    /// ```baml
    /// [1.0, 2.0, 3.0, 4.0].mean()   // 2.5
    /// [].mean()                     // throws InvalidArgument
    /// ```
    function mean(self) -> float throws root.errors.InvalidArgument {
        if (self.length() == 0) {
            throw root.errors.InvalidArgument { message: "float[].mean: cannot take the mean of an empty array" };
        }
        Summable.sum(self) / self.length()
    }

    /// Returns the median (the middle value) of `self`.
    ///
    /// The elements are copied and sorted ascending by IEEE 754 `totalOrder`,
    /// matching `float[].sort()`, so the caller's array is left untouched. An
    /// odd element count yields the single middle element; an even count yields
    /// the arithmetic mean of the two middle elements, computed so the
    /// intermediate sum cannot overflow. `totalOrder` gives `NaN` a defined
    /// position at the extremes, so a `NaN` element shifts the middle rather
    /// than poisoning the result.
    ///
    /// # Throws
    /// - `baml.errors.InvalidArgument` when `self` is empty.
    ///
    /// # Examples
    /// ```baml
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

/// (internal) Native backing for `int[].sum()`; call that instead. Adds every
/// element left to right starting from `0`, range-checking the running total at
/// each step. The empty array sums to `0`.
///
/// # Panics
/// - `baml.panics.IntegerOverflow` when a running total leaves the `int` range.
//baml:fallible
function _sum_int(values: int[]) -> int throws never {
    $rust_function
}

/// (internal) Native backing for `float[].sum()`; call that instead. Adds every
/// element left to right starting from `0.0` with plain sequential IEEE 754
/// addition; the empty array sums to `0.0`. Never fails — a total too large to
/// represent becomes an infinity.
//baml:vm
function _sum_float(values: float[]) -> float throws never {
    $rust_function
}

/// (internal) Native backing for `float[].median()`; call that instead. Sorts a
/// *copy* of `values` by IEEE 754 `totalOrder` and returns the middle element,
/// or the mean of the two middle elements for an even count, leaving `values`
/// untouched.
///
/// # Throws
/// - `baml.errors.InvalidArgument` when `values` is empty.
//baml:vm
function _median_float(values: float[]) -> float throws root.errors.InvalidArgument {
    $rust_function
}
