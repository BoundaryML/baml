---
title: "Uint8Array"
description: "Class Uint8Array from the generated baml package reference."
---

A mutable, growable array of bytes (u8 values in the range 0–255).

Used for binary data such as file contents, network payloads, and encoded strings.
`push` silently masks values to u8; `from_array` throws `InvalidArgument` for out-of-range values.

```baml
class Uint8Array
```

## Methods

### _to_string_impl

```baml
function _to_string_impl(self: uint8array) -> string
```

No description is available yet.

### at

```baml
function at(self: uint8array, index: int) -> int | null
```

Returns the byte at `index`, or `null` if out of bounds.

Negative indices count from the end: `-1` is the last byte.

### concat

```baml
function concat(self: uint8array, other: uint8array) -> uint8array
```

Returns a new `uint8array` containing the concatenation of the two arrays.

### from_array

```baml
function from_array(array: int[]) -> uint8array throws baml.errors.InvalidArgument
```

Creates a `uint8array` from an array of integers.

Throws `InvalidArgument` if any value is outside the range 0–255.

### from_base64

```baml
function from_base64(base64: string) -> uint8array throws baml.errors.InvalidArgument
```

Decodes a standard Base64-encoded string into bytes.

Accepts both standard (`+/`) and URL-safe (`-_`) alphabets, with or without padding.

### Examples
```
uint8array.from_base64("aGVsbG8=")   // [104, 101, 108, 108, 111]  ("hello")
```

Throws `InvalidArgument` if the input is not valid Base64.

### from_hex

```baml
function from_hex(hex: string) -> uint8array throws baml.errors.InvalidArgument
```

Decodes a hexadecimal string (e.g. `"deadbeef"`) into bytes.

Throws `InvalidArgument` if the input contains non-hex characters or has an odd length.

### includes

```baml
function includes(self: uint8array, item: int) -> bool
```

Returns `true` if the array contains the given number.
If the item is not a `u8`, this will always return `false`.

### index_of

```baml
function index_of(self: uint8array, item: int) -> int | null
```

Returns the index of the first occurrence of `item`, or `null` when it
is absent. An `item` outside the `u8` range is never present.

### length

```baml
function length(self: uint8array) -> int
```

Returns the number of bytes in the array.

### pop

```baml
function pop(self: uint8array) -> int | null
```

No description is available yet.

### push

```baml
function push(self: uint8array, item: int) -> int
```

No description is available yet.

### reverse

```baml
function reverse(self: uint8array) -> uint8array
```

Returns a new `uint8array` with the bytes in reverse order.

### slice

```baml
function slice(self: uint8array, start: int, end: int) -> uint8array
```

Returns a new `uint8array` with the bytes from `start` (inclusive) to
`end` (exclusive).

Negative indices count from the end. Out-of-range indices are clamped, and
an `end` that resolves at or before `start` yields an empty array.

### sort

```baml
function sort(self: uint8array) -> null
```

No description is available yet.

### to_array

```baml
function to_array(self: uint8array) -> int[]
```

Returns the bytes as an array of integers, each in the range 0–255.

### to_base64

```baml
function to_base64(self: uint8array) -> string
```

Encodes the bytes as a standard Base64 string (with `=` padding).

### Examples
```
"hello".to_utf8().to_base64()   // "aGVsbG8="
```

### to_hex

```baml
function to_hex(self: uint8array) -> string
```

Encodes the bytes as a lowercase hexadecimal string (e.g. `"deadbeef"`).

### zeroes

```baml
function zeroes(size: int) -> uint8array throws baml.errors.InvalidArgument
```

Creates a new `uint8array` of the given size, filled with zeros.

Throws an error if the `size` is out of range (e.g. negative).
Panics if the allocation would cause an OOM.

_Source: `<builtin>/baml/uint8array.baml:267`_
