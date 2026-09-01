---
title: "Array"
description: "Class Array from the generated baml package reference."
---

An ordered, growable collection of elements of type T.

```baml
class Array<T>
```

## Methods

### at

```baml
function at(self: T[], index: int) -> T | null
```

Returns the element at `index`, or `null` if out of bounds.

Negative indices count from the end: `-1` is the last element.

### Examples
```
[10, 20, 30].at(0)    // 10
[10, 20, 30].at(-1)   // 30
[10, 20, 30].at(99)   // null
[10, 20, 30].at(-99)  // null
```

### clear

```baml
function clear(self: T[]) -> null
```

No description is available yet.

### concat

```baml
function concat(self: T[], other: T[]) -> T[]
```

Returns a new array with the elements of `self` followed by the elements of `other`.

### every

```baml
function every<E>(self: T[], predicate: (T) -> bool throws E) -> bool throws E
```

No description is available yet.

### filled

```baml
function filled(length: int, value: T) -> T[]
```

Builds a new array of `length` elements, each equal to `value`.

This is the idiomatic way to allocate a runtime-sized, pre-initialized
buffer — Fenwick trees, DP tables, adjacency lists — without a manual
`while`/`push` loop. (Remember a 1-indexed structure needs `length + 1`
slots.)

### Parameters
- `length`: the number of elements to create. A negative or zero `length`
  produces an empty array.
- `value`: the value stored in every slot.

### Warning
`Array.filled` reuses the exact same `value` for every slot. For reference
types (arrays, maps, class instances), every slot aliases the same object.
Mutating one slot mutates all slots.

To get independent per-slot values, use `Array.generate`, which calls a
factory once per index:
```
Array.generate(3, (i: int) -> int[] { [] })   // three independent []s
```

### Returns
A fresh `T[]` of length `max(length, 0)` with every element equal to
`value`.

Never throws.

### Examples
```
Array.filled(3, 0)     // [0, 0, 0]
Array.filled(0, 0)     // []
Array.filled(-1, 0)    // []
Array.filled(2, "x")   // ["x", "x"]
Array.filled(3, [])    // aliases the same inner array in all 3 slots
```

### filter

```baml
function filter<E>(self: T[], predicate: (T) -> bool throws E) -> T[] throws E
```

No description is available yet.

### filter_map

```baml
function filter_map<U, E>(self: T[], fn: (T) -> U | null throws E) -> U[] throws E
```

Returns a new array containing the non-null results of `fn` applied to
every element. The function is called once per element, left-to-right.
Does not mutate `self`. For a lazy version, use `iter().filter_map(fn)`.

### find

```baml
function find<E>(self: T[], predicate: (T) -> bool throws E) -> T | null throws E
```

No description is available yet.

### find_index

```baml
function find_index<E>(self: T[], predicate: (T) -> bool throws E) -> int | null throws E
```

No description is available yet.

### find_last

```baml
function find_last<E>(self: T[], predicate: (T) -> bool throws E) -> T | null throws E
```

No description is available yet.

### find_last_index

```baml
function find_last_index<E>(self: T[], predicate: (T) -> bool throws E) -> int | null throws E
```

No description is available yet.

### flat_map

```baml
function flat_map<U, E>(self: T[], f: (T) -> U[] throws E) -> U[] throws E
```

No description is available yet.

### for_each

```baml
function for_each<E>(self: T[], fn: (T) -> void throws E) -> void throws E
```

Calls `fn` once for each element in the array.

### generate

```baml
function generate<E>(length: int, f: (int) -> T throws E) -> T[] throws E
```

Builds a new array of `length` elements by calling `f` once per index.

`f` is invoked with each index `0, 1, ..., length - 1` in order, and its
result is stored in that slot. Unlike `Array.filled`, which reuses one
shared value, `generate` produces an **independent value per slot** — the
alias-free way to build runtime-sized buffers of reference types (rows of a
grid, per-slot maps or class instances). It mirrors JavaScript's
`Array.from({ length }, f)` and a Python list comprehension.

### Parameters
- `length`: the number of elements to create. A negative or zero `length`
  produces an empty array and never calls `f`.
- `f`: called once per index to produce that slot's value. Any error it
  throws propagates to the caller, halting generation.

### Returns
A fresh `T[]` of length `max(length, 0)` whose element `i` is `f(i)`.

### Examples
```
Array.generate(3, (i: int) -> int { i * i })      // [0, 1, 4]
Array.generate(0, (i: int) -> int { i })          // []
// A 2D grid with independent rows (mutating one row leaves the rest):
Array.generate(rows, (r: int) -> int[] { Array.filled(cols, 0) })
```

### includes

```baml
function includes(self: T[], item: T) -> bool
```

No description is available yet.

### index_of

```baml
function index_of(self: T[], item: T) -> int | null
```

No description is available yet.

### insert

```baml
function insert(self: T[], item: T, idx: int) -> int throws baml.errors.InvalidArgument
```

No description is available yet.

### join

```baml
function join(self: T[], separator: string) -> string
```

Joins all elements into a string, separated by `separator`. Each element is converted to a string via its `to_string` representation.

### last_index_of

```baml
function last_index_of(self: T[], item: T) -> int | null
```

No description is available yet.

### length

```baml
function length(self: T[]) -> int
```

Returns the number of elements in the array.

### map

```baml
function map<U, E>(self: T[], f: (T) -> U throws E) -> U[] throws E
```

No description is available yet.

### pop

```baml
function pop(self: T[]) -> T | null
```

No description is available yet.

### push

```baml
function push(self: T[], item: T) -> int
```

No description is available yet.

### reduce

```baml
function reduce<A, E>(self: T[], reducer: (A, T) -> A throws E, initial: A) -> A throws E
```

No description is available yet.

### remove_at

```baml
function remove_at(self: T[], index: int) -> T | null
```

No description is available yet.

### reverse

```baml
function reverse(self: T[]) -> T[]
```

Returns a new array with the elements in reverse order. Does not mutate `self`.

### shift

```baml
function shift(self: T[]) -> T | null
```

No description is available yet.

### slice

```baml
function slice(self: T[], start: int, end: int) -> T[]
```

Returns a sub-array from index `start` (inclusive) to `end` (exclusive).

Negative indices count from the end. Out-of-range indices are clamped, and
an `end` that resolves at or before `start` yields an empty array.

### Examples
```
[1, 2, 3, 4].slice(1, 3)   // [2, 3]
[1, 2, 3].slice(-2, 3)     // [2, 3]
[1, 2, 3].slice(2, 1)      // []
```

### some

```baml
function some<E>(self: T[], predicate: (T) -> bool throws E) -> bool throws E
```

No description is available yet.

### sort_by

```baml
function sort_by<E>(self: T[], compare: (T, T) -> int throws E) -> T[] throws E
```

No description is available yet.

### sort_by_key

```baml
function sort_by_key<U extends baml.Comparable, E>(self: T[], key: (T) -> U throws E) -> T[] throws E | !error
```

No description is available yet.

### splice

```baml
function splice(self: T[], start: int, count: int, replace: T[]) -> null throws baml.errors.InvalidArgument
```

No description is available yet.

### unshift

```baml
function unshift(self: T[], item: T) -> int
```

No description is available yet.

_Source: `<builtin>/baml/containers.baml:59`_
