/// An ordered, growable collection of elements of type T.
class Array<T> {
  /// Returns the number of elements in the array.
  function length(self) -> int {
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
  /// ```
  function at(self, index: int) -> T? {
    $rust_function
  }

  /// Appends `item` to the end of the array. Returns the new length. Mutates `self`.
  //baml:mut_self
  function push(self, item: T) -> int {
    $rust_function
  }

  /// Removes and returns the last element, or `null` if the array is empty. Mutates `self`.
  //baml:mut_self
  function pop(self) -> T? {
    $rust_function
  }

  /// Returns a new array with the elements of `self` followed by the elements of `other`.
  function concat(self, other: T[]) -> T[] {
    $rust_function
  }

  /// Returns a new array with the elements in reverse order. Does not mutate `self`.
  function reverse(self) -> T[] {
    $rust_function
  }

  /// Returns a sub-array from index `start` (inclusive) to `end` (exclusive).
  ///
  /// Negative indices count from the end. Out-of-range indices are clamped.
  ///
  /// # Examples
  /// ```
  /// [1, 2, 3, 4].slice(1, 3)   // [2, 3]
  /// [1, 2, 3].slice(-2, 3)     // [2, 3]
  /// ```
  function slice(self, start: int, end: int) -> T[] {
    $rust_function
  }

  /// Joins all elements into a string, separated by `separator`. Each element is converted to a string via its `to_string` representation.
  //baml:vm
  function join(self, separator: string) -> string {
    $rust_function
  }

  /// Applies `f` to each element and returns an array of the results.
  //baml:mut_vm
  //baml:may_yield
  function map<U, E>(self, f: (T) -> U throws E) -> U[] throws E {
    $rust_function
  }

  /// Serializes the array to a JSON array. Each element must be JSON-serializable.
  //baml:mut_vm
  //baml:may_yield
  function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
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
  function includes(self, item: T) -> bool {
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
  function index_of(self, item: T) -> int? {
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
  function last_index_of(self, item: T) -> int? {
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
  function clear(self) -> null {
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
  function shift(self) -> T? {
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
  function unshift(self, item: T) -> int {
    $rust_function
  }

  /// Inserts `item` at index `idx`, shifting existing elements at
  /// `[idx, length())` one position to the right. Returns the new length.
  /// Mutates `self` in place.
  ///
  /// `idx` may equal `length()`, in which case the call is equivalent to
  /// `push(item)`. Throws `InvalidArgument` if `idx < 0` or `idx > length()`.
  ///
  /// # Examples
  /// ```
  /// let xs = [1, 2, 4];
  /// xs.insert(3, 2)        // 4   (new length)
  /// xs                     // [1, 2, 3, 4]
  /// xs.insert(99, xs.length())  // append; new length 5
  /// xs.insert(0, -1)       // throws — negative idx
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
  /// Throws `InvalidArgument` if `start < 0` or `start > length()`. If
  /// `start + count` would exceed `length()`, `count` is clamped to
  /// `length() - start`. `count` must be non-negative.
  ///
  /// # Examples
  /// ```
  /// let xs = [0, 1, 2, 3, 4];
  /// xs.splice(1, 2, [9, 9, 9])   // xs is now [0, 9, 9, 9, 3, 4]
  /// xs.splice(0, 0, [-1])        // pure insertion: [-1, 0, 9, 9, 9, 3, 4]
  /// xs.splice(0, 1, [])          // pure removal: [0, 9, 9, 9, 3, 4]
  /// xs.splice(99, 0, [])         // throws — start out of range
  /// ```
  //baml:mut_self
  function splice(self, start: int, count: int, replace: T[]) -> null throws root.errors.InvalidArgument {
    $rust_function
  }
}

/// An unordered collection of key-value pairs.
class Map<K, V> {
  /// Returns the number of entries in the map.
  function length(self) -> int {
    $rust_function
  }

  /// Returns `true` if the map contains an entry for `key`.
  //baml:vm
  function has(self, key: K) -> bool {
    $rust_function
  }

  /// Returns an array of all keys in the map. Order is not guaranteed.
  //baml:mut_vm
  function keys(self) -> K[] {
    $rust_function
  }

  /// Returns an array of all values in the map. Order matches `keys()`.
  function values(self) -> V[] {
    $rust_function
  }

  /// Inserts or replaces the entry for `key`. Returns the previously
  /// associated value, or `null` if `key` was not already present.
  ///
  /// # Examples
  /// ```
  /// let m: map<string, int> = {};
  /// m.set("a", 1)         // null  (was absent)
  /// m.set("a", 2)         // 1     (returns the old value)
  /// m.get("a")            // 2
  /// ```
  //baml:mut_self
  function set(self, key: K, value: V) -> V? {
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
  function get(self, key: K) -> V? {
    $rust_function
  }

  /// Serializes the map to a JSON object. Keys must be strings.
  //baml:mut_vm
  //baml:may_yield
  function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError | baml.json.JsonParseError {
    $rust_function
  }

  /// Removes and returns the entry for `key`. Returns `null` if the key was
  /// not present.
  ///
  /// # Examples
  /// ```
  /// let m: map<string, int> = {"a": 1, "b": 2};
  /// m.delete("a")     // 1
  /// m.delete("a")     // null  (already gone)
  /// m.length()        // 1
  /// ```
  //baml:mut_self
  function delete(self, key: K) -> V? {
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
  function get_or_insert(self, key: K, default: V) -> V {
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
  function clear(self) -> null {
    $rust_function
  }
}
