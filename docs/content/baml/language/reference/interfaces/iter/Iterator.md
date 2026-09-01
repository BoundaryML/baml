---
title: "iter.Iterator"
description: "Interface iter.Iterator from the generated baml package reference."
---

Implemented by types as a way to iterate over a sequence of values,
often from some container.

```baml
interface iter.Iterator
```

## Associated types

### Item

```baml
type Item
```

No description is available yet.

### Error

```baml
type Error
```

No description is available yet.

## Required methods

### next

```baml
function next(self: Self) -> (Self as baml.iter.Iterator).Item | baml.iter.Done throws (Self as baml.iter.Iterator).Error
```

Advances the iterator and returns the next value.
Returns `baml.iter.Done` when iteration is finished.

## Default methods

### chain

```baml
function chain<E2>(self: Self, other: baml.iter.Iterable<Error = E2, Item = (Self as baml.iter.Iterator).Item>) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that represents the concatenation of two iterators.
It will yield all elements in `self` then all elements in the iterator of `other`.

Iterators are lazy (pull-based): the original iterators will not be advanced
unless something calls `next` on the produced iterator.

### collect

```baml
function collect(self: Self) -> (Self as baml.iter.Iterator).Item[] throws !error
```

Consumes the rest of this iterator and returns an array with all elements.

Do not call this method on an infinite iterator as it will never terminate.

### count

```baml
function count(self: Self) -> int throws !error
```

Consumes the rest of this iterator and counts the number of elements.

Do not call this method on an infinite iterator as it will never terminate.

### every

```baml
function every<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> bool throws !error | E2
```

Returns `true` if the predicate returns `true` for all of the elements in the iterator.
Short circuiting: will only consume elements up to the first `false` result.

ALIASES: all

### filter

```baml
function filter<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator which uses the provided function to decide if an element should be yielded.
If the predicate returns `true` then the element is yielded, otherwise it will skip the element
and try the next one.

Iterators are lazy (pull-based): the original iterator will not be advanced
and the provided function will not be called unless something calls `next` on the produced iterator.

### filter_map

```baml
function filter_map<R, E2>(self: Self, fn: ((Self as baml.iter.Iterator).Item) -> R | null throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = R>
```

A combination of `baml.iter.Iterator.map` and `baml.iter.Iterator.filter`:
If the provided function returns `null` then the element will be skipped, otherwise the
produced iterator will yield the result of the provided function.

Iterators are lazy (pull-based): the original iterator will not be advanced
and the provided function will not be called unless something calls `next` on the produced iterator.

### find

```baml
function find<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> (Self as baml.iter.Iterator).Item | null throws !error | E2
```

Returns the first element where the predicate returns `true`, or `null` if none do.
Short circuiting: will only consume elements up to the first `true` result.

### flat_map

```baml
function flat_map<R, E2, E3>(self: Self, fn: ((Self as baml.iter.Iterator).Item) -> baml.iter.Iterable<Error = E3, Item = R> throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2 | E3, Item = R>
```

A combination of `baml.iter.Iterator.map` and `baml.iter.flatten`:
For each element in the original iterator, the provided function will be called to get some iterable object.
The produced iterator will then yield each element in that object's iterator and will only consume the
next element of the original iterator once the object's iterator is exhausted.

Iterators are lazy (pull-based): the original iterator will not be advanced
and the provided function will not be called unless something calls `next` on the produced iterator.

### for_each

```baml
function for_each<E2>(self: Self, fn: ((Self as baml.iter.Iterator).Item) -> void throws E2) -> void throws !error | E2
```

Consumes the rest of this iterator and calls the provided function on each element.
It is generally preferred to use a `for (.. in ..)` loop instead, but
this method may be useful at the end of a long chain of calls.

### map

```baml
function map<R, E2>(self: Self, fn: ((Self as baml.iter.Iterator).Item) -> R throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = R>
```

Creates a new iterator which calls the provided function on each element.

Iterators are lazy (pull-based): the original iterator will not be advanced
and the provided function will not be called unless something calls `next` on the produced iterator.

### peekable

```baml
function peekable(self: Self) -> baml.iter.Peekable<(Self as baml.iter.Iterator).Item, (Self as baml.iter.Iterator).Error>
```

Creates a new peekable iterator from this iterator.

### reduce

```baml
function reduce<A, E2>(self: Self, fn: (A, (Self as baml.iter.Iterator).Item) -> A throws E2, initial: A) -> A throws !error | E2
```

Consumes the rest of this iterator and for each element, calls the provided function
with the provided accumulator.

Do not call this method on an infinite iterator as it will never terminate.

ALIASES: fold

### skip

```baml
function skip(self: Self, n: int) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that discards the first `n` elements of the original
iterator and yields the rest.

For `n <= 0`, every element is yielded. If the source has fewer than `n`
elements, the new iterator yields nothing.

```
baml.iter.Range.new(0, 5).skip(2).collect() // [2, 3, 4]
```

Iterators are lazy (pull-based): the elements are not skipped until
something calls `next` on the produced iterator.

### skip_while

```baml
function skip_while<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that discards leading elements while `predicate`
returns `true`, then yields every remaining element.

The predicate is only consulted until it first returns `false`; after that
every element is yielded, including ones the predicate would have matched:

```
// stops skipping at 3, so the trailing 1 and 2 are kept
[1, 2, 3, 1, 2].iter().skip_while((x: int) -> bool { x < 3 }).collect() // [3, 1, 2]
```

Iterators are lazy (pull-based): the original iterator will not be advanced
and the predicate will not be called unless something calls `next` on the
produced iterator.

### some

```baml
function some<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> bool throws !error | E2
```

Returns `true` if the predicate returns `true` for any of the elements in the iterator.
Short circuiting: will only consume elements up to the first `true` result.

ALIASES: any

### step_by

```baml
function step_by(self: Self, n: int) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that will only yield every `n` elements of the original iterator.
For `n <= 1`, the new iterator will yield every element.

Iterators are lazy (pull-based): the original iterator will not be advanced
unless something calls `next` on the produced iterator.

### take

```baml
function take(self: Self, n: int) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that yields at most the first `n` elements.

Once `n` elements have been yielded the source is no longer advanced.
For `n <= 0`, the new iterator yields nothing. If the source ends first,
the new iterator ends with it.

This is the usual way to bound an unbounded iterator such as
`Repeat.new(v)` or a wide `Range`:

```
baml.iter.Repeat.new(0).take(5).collect()   // [0, 0, 0, 0, 0]
baml.iter.Range.new(0, 10).take(3).collect() // [0, 1, 2]
```

Iterators are lazy (pull-based): the original iterator will not be advanced
unless something calls `next` on the produced iterator.

### take_while

```baml
function take_while<E2>(self: Self, predicate: ((Self as baml.iter.Iterator).Item) -> bool throws E2) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterator).Error | E2, Item = (Self as baml.iter.Iterator).Item>
```

Creates a new iterator that yields elements while `predicate` returns `true`,
and stops at the first element for which it returns `false`.

The element that fails the predicate is consumed from the source and is not
yielded. Unlike `filter`, iteration **stops** at the first failure rather
than skipping it:

```
// stops at 5, so 6 and 8 are never yielded
[2, 4, 5, 6, 8].iter().take_while((x: int) -> bool { x % 2 == 0 }).collect() // [2, 4]
```

Iterators are lazy (pull-based): the original iterator will not be advanced
and the predicate will not be called unless something calls `next` on the
produced iterator.

_Source: `<builtin>/baml/ns_iter/iter.baml:631`_
