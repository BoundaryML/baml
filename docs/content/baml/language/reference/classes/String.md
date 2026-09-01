---
title: "String"
description: "Class String from the generated baml package reference."
---

A UTF-8 encoded string.

Quoted strings may span multiple lines. Backtick strings support interpolation.
All methods returning a new string do not mutate `self` unless noted otherwise.

```baml
class String
```

## Methods

### at

```baml
function at(self: string, index: int) -> string | null
```

Returns the character at the given **codepoint index** as a
one-character string.

Negative indices count from the end: `-1` is the last character.
Returns `null` if the index is out of range.

### Examples
```
"hello".at(0)   // "h"
"héllo".at(1)   // "é"
"😀hello".at(0) // "😀"
"hello".at(-1)  // "o"
"hi".at(99)     // null
```

### byte_length

```baml
function byte_length(self: string) -> int
```

Returns the length of the string in **UTF-8 bytes**.

This is `O(1)`. Useful for serialization, network I/O, and buffer
allocation where byte size matters.

### Examples
```
"hello".byte_length()  // 5
"héllo".byte_length()  // 6
"😀".byte_length()     // 4
```

### char_count

```baml
function char_count(self: string) -> int
```

Alias for `length()`.

### chars

```baml
function chars(self: string) -> string[]
```

Returns the string's Unicode code points as one-character strings.

### Examples
```
"hello".chars()  // ["h", "e", "l", "l", "o"]
"é😀".chars()    // ["é", "😀"]
"".chars()       // []
```

### code_point_at

```baml
function code_point_at(self: string, index: int) -> int | null
```

Returns the **Unicode code point** of the character at the given codepoint
index, as an `int`.

This is the numeric counterpart of `at`, which returns the character as a
one-character string. Negative indices count from the end: `-1` is the last
character. Returns `null` if the index is out of range.

The returned value is always in `[0, 0x10FFFF]`, so it round-trips through
`string.from_code_points([cp])`.

### Examples
```
"A".code_point_at(0)        // 65
"é".code_point_at(0)        // 233
"🐑".code_point_at(0)       // 128017
"hello".code_point_at(-1)   // 111 ("o")
"😀hello".code_point_at(1)  // 104 ("h", not a UTF-16 unit)
"hi".code_point_at(99)      // null
```

### ends_with

```baml
function ends_with(self: string, suffix: string) -> bool
```

Returns true if the string ends with `suffix`.

### from

```baml
function from<T>(value: T) -> string
```

Renders any value as a human-readable string.

If `value`'s runtime type implements `baml.ToString`, its `to_string`
override is used; otherwise a default structural rendering is produced.
Never throws.

```
string.from(42)            // "42"
string.from(true)          // "true"
string.from([1, 2, 3])     // "[1, 2, 3]"
```

Ideally this would read:

```
if (value is baml.ToString) { value.to_string() }
else { root._to_string_default(value) }
```

but that does not work from the stdlib today. `is <interface>` and
interface method dispatch are resolved at *compile time* by expanding the
interface to its known implementor classes, and that implementor set is
per-package: it is built from the package's dependencies + itself, never
its dependents. `string.from` lives in the `baml` package, which a user's
`implements baml.ToString` sits *above*, so from here the implementor set
for `baml.ToString` is empty — the `is` test folds to constant `false` and
the override never dispatches. (Same boundary `Sortable.sort`'s
`_compare_shim` and `baml.json.to_json` work around.)

So dispatch is resolved on `value`'s *runtime* class via
`baml._to_string_shim` instead. TODO: once Kai's TIR rework lands (it moves
interface conformance checks to runtime), replace this with the literal
`is baml.ToString` form above and drop `_to_string_shim`.

### from_code_points

```baml
function from_code_points(unicode: int[]) -> string throws baml.errors.InvalidArgument
```

Builds a string from an array of Unicode code points.

Each value in `unicode` must be in the range `[0, 0x10FFFF]` and must
not be a UTF-16 surrogate (the range `[0xD800, 0xDFFF]`). Throws
`InvalidArgument` on any invalid value, identifying the offending
position in the array.

### Examples
```
string.from_code_points([104, 105])         // "hi"
string.from_code_points([233])              // "é"
string.from_code_points([128017])           // "🐑"
string.from_code_points([-1])               // throws — out of range
string.from_code_points([55296])            // throws — surrogate (U+D800)
```

### from_utf8

```baml
function from_utf8(utf8: uint8array) -> string throws baml.errors.InvalidArgument
```

Decodes a `uint8array` of UTF-8 bytes into a string.

Throws `InvalidArgument` if `utf8` is not valid UTF-8. To decode bytes
with replacement characters (lossy), use `uint8array.to_string()`
instead, which substitutes U+FFFD for invalid sequences.

### Examples
```
string.from_utf8(b"\x68\x69")          // "hi"
string.from_utf8(b"\xC3\xA9")          // "é"
string.from_utf8(b"\xFF")              // throws — 0xFF is invalid UTF-8
```

### includes

```baml
function includes(self: string, search: string) -> bool
```

Returns true if `search` appears anywhere in the string.

### index_of

```baml
function index_of(self: string, search: string) -> int | null
```

Returns the **character index** of the first occurrence of `search`, or
`null` if not found.

### Examples
```
"hello world".index_of("world")  // 6
"héllo".index_of("l")            // 2
"😀hello".index_of("h")          // 1
"abc".index_of("z")              // null
```

### is_alphabetic

```baml
function is_alphabetic(self: string) -> bool
```

Returns true if every character is a Unicode letter (general category `L`).
Empty string returns true.

### Examples
```
"héllo".is_alphabetic()  // true
"漢字".is_alphabetic()    // true
"abc1".is_alphabetic()   // false
```

### is_alphanumeric

```baml
function is_alphanumeric(self: string) -> bool
```

Returns true if every character is alphabetic OR numeric per Unicode.
Empty string returns true. Equivalent to checking each char passes
`is_alphabetic` or `is_numeric`.

### Examples
```
"abc123".is_alphanumeric()  // true
"héllo7".is_alphanumeric()  // true
"a b".is_alphanumeric()     // false (space is not)
```

### is_ascii

```baml
function is_ascii(self: string) -> bool
```

Returns true if every character is ASCII (i.e. has a code point in
`[0x00, 0x7F]`). Empty string returns true.

### Examples
```
"hello".is_ascii()    // true
"héllo".is_ascii()    // false (`é` is U+00E9)
```

### is_ascii_alphabetic

```baml
function is_ascii_alphabetic(self: string) -> bool
```

Returns true if every character is an ASCII letter (`A`..=`Z` or
`a`..=`z`). Empty string returns true.

### is_ascii_alphanumeric

```baml
function is_ascii_alphanumeric(self: string) -> bool
```

Returns true if every character is an ASCII letter or digit. Empty
string returns true.

### is_ascii_control

```baml
function is_ascii_control(self: string) -> bool
```

Returns true if every character is an ASCII control character — code
points `[0x00, 0x1F]` plus DEL (`0x7F`). Empty string returns true.

### is_ascii_graphic

```baml
function is_ascii_graphic(self: string) -> bool
```

Returns true if every character is an ASCII "graphic" character —
printable, non-space, non-control: code points `[0x21, 0x7E]`. Note
the ASCII space is *not* graphic. Empty string returns true.

### is_ascii_hex

```baml
function is_ascii_hex(self: string) -> bool
```

Returns true if every character is an ASCII hexadecimal digit:
`0`..=`9`, `a`..=`f`, or `A`..=`F`. Empty string returns true.

### Examples
```
"deadBEEF".is_ascii_hex()  // true
"0x123".is_ascii_hex()     // false (`x` is not a hex digit)
```

### is_ascii_lowercase

```baml
function is_ascii_lowercase(self: string) -> bool
```

Returns true if every character is an ASCII lowercase letter
(`a`..=`z`). Empty string returns true.

### is_ascii_numeric

```baml
function is_ascii_numeric(self: string) -> bool
```

Returns true if every character is an ASCII decimal digit (`0`..=`9`).
Empty string returns true. Stricter than `is_numeric`, which accepts
non-ASCII numerals.

### Examples
```
"12345".is_ascii_numeric()  // true
"Ⅷ".is_ascii_numeric()      // false (Roman numeral)
```

### is_ascii_uppercase

```baml
function is_ascii_uppercase(self: string) -> bool
```

Returns true if every character is an ASCII uppercase letter
(`A`..=`Z`). Empty string returns true.

### is_ascii_whitespace

```baml
function is_ascii_whitespace(self: string) -> bool
```

Returns true if every character is ASCII whitespace: space (`\x20`),
horizontal tab (`\t`), line feed (`\n`), form feed (`\x0C`), or
carriage return (`\r`). Note this is narrower than `is_whitespace`,
which accepts Unicode whitespace like NBSP.

### is_control

```baml
function is_control(self: string) -> bool
```

Returns true if every character is a control character per Unicode
(general category `Cc`). Empty string returns true.

### Examples
```
"\n\t".is_control()  // true
"a".is_control()     // false
```

### is_empty

```baml
function is_empty(self: string) -> bool
```

Returns true if the string has no characters.

### Examples
```
"".is_empty()       // true
"hello".is_empty()  // false
```

### is_graphic

```baml
function is_graphic(self: string) -> bool
```

Returns true if every character is a "graphic" character — that is,
not a control character and not whitespace. Empty string returns true.

This is a convenience predicate for "visible/printing" characters. It
uses the Unicode definition of control and whitespace.

### Examples
```
"abc".is_graphic()      // true
"héllo".is_graphic()    // true
"a b".is_graphic()      // false (space is not graphic)
"a\n".is_graphic()      // false (newline is control)
```

### is_lowercase

```baml
function is_lowercase(self: string) -> bool
```

Returns true if every character is lowercase per Unicode (general
category `Ll`, plus other chars with the `Lowercase` property).
Empty string returns true.

### Examples
```
"hello".is_lowercase()  // true
"Hello".is_lowercase()  // false
```

### is_numeric

```baml
function is_numeric(self: string) -> bool
```

Returns true if every character is numeric per Unicode (general categories
`Nd`, `Nl`, `No` — decimal digits, letter-numbers, and other-numbers).
Empty string returns true.

### Examples
```
"12345".is_numeric()  // true
"Ⅷ".is_numeric()      // true (Roman numeral, Nl)
"12a".is_numeric()    // false
"".is_numeric()       // true (vacuous)
```

### is_uppercase

```baml
function is_uppercase(self: string) -> bool
```

Returns true if every character is uppercase per Unicode (general
category `Lu`, plus other chars with the `Uppercase` property).
Empty string returns true. Note: digits and most symbols are neither
upper- nor lowercase.

### Examples
```
"HELLO".is_uppercase()  // true
"Hello".is_uppercase()  // false
"123".is_uppercase()    // false (digits aren't uppercase)
```

### is_whitespace

```baml
function is_whitespace(self: string) -> bool
```

Returns true if every character is whitespace per Unicode (`White_Space`
property — includes space, tab, newline, NBSP, and other Unicode
whitespace). Empty string returns true.

### Examples
```
"   ".is_whitespace()      // true
" \t\n".is_whitespace()    // true
"a b".is_whitespace()      // false
```

### last_index_of

```baml
function last_index_of(self: string, search: string) -> int | null
```

Returns the **character index** of the start of the last occurrence of `search`, or
`null` if not found.

### Examples
```
"hello world".last_index_of("world")  // 6
"héllo".last_index_of("l")            // 2
"😀hello".last_index_of("h")          // 1
"abc".last_index_of("z")              // null
```

### length

```baml
function length(self: string) -> int
```

Returns the number of **Unicode code points** (characters) in the string.

This is `O(1)` — the count is cached at construction time. For ASCII
text it equals `byte_length()`, but for strings with multi-byte UTF-8
characters they differ.

### Examples
```
"hello".length()  // 5
"héllo".length()  // 5
"😀".length()     // 1
"".length()       // 0
```

### lines

```baml
function lines(self: string) -> string[]
```

Splits the string into lines, recognizing both `\n` and `\r\n` as line
terminators. The terminator is **not** included in the returned strings.
A final terminator does not produce a trailing empty string.

### Examples
```
"a\nb\nc".lines()    // ["a", "b", "c"]
"a\nb\n".lines()     // ["a", "b"]      (no trailing empty)
"a\r\nb".lines()     // ["a", "b"]      (CRLF handled)
"".lines()           // []
"\n".lines()         // [""]
```

### repeat

```baml
function repeat(self: string, count: int) -> string
```

Returns a new string that repeats `self` the given number of times.
Negative counts are treated as 0.

### Examples
```
"ab".repeat(3)   // "ababab"
"hi".repeat(0)   // ""
"hi".repeat(-1)  // ""
```

### replace

```baml
function replace(self: string, search: string, replacement: string) -> string
```

Replaces the first occurrence of `search` with `replacement`.

### replace_all

```baml
function replace_all(self: string, search: string, replacement: string) -> string
```

Replaces all occurrences of `search` with `replacement`.

### slice

```baml
function slice(self: string, start: int, end: int) -> string
```

Returns the substring between two **character** offsets `[start, end)`.

Both `start` and `end` are codepoint indices, matching `length()`. Negative
indices count from the end. The resolved offsets are clamped to
`[0, length()]`, and an `end` that resolves at or before `start` yields an
empty string. Never throws.

### Examples
```
"hello world".slice(0, 5)   // "hello"
"hello".slice(2, 100)       // "llo" (end clamped)
"hello".slice(-3, -1)       // "ll" (counts from the end)
"😀hello".slice(0, 1)       // "😀"
"😀hello".slice(1, 4)       // "hel"
```

### split

```baml
function split(self: string, delimiter: string) -> string[]
```

Splits the string by `delimiter` and returns an array of substrings.

### Examples
```
"a,b,c".split(",")     // ["a", "b", "c"]
"hello".split("")      // ["h", "e", "l", "l", "o"]
"no match".split(",")  // ["no match"]
```

### starts_with

```baml
function starts_with(self: string, prefix: string) -> bool
```

Returns true if the string starts with `prefix`.

### to_code_points

```baml
function to_code_points(self: string) -> int[]
```

Returns the string's Unicode code points as an array of `int`s.

This is the exact inverse of `string.from_code_points`: for any string
`s`, `string.from_code_points(s.to_code_points())` equals `s`. The result
has one element per character (`self.length()` elements), each in
`[0, 0x10FFFF]`. Never throws.

Use this for char → integer mappings (checksums, base-N encoding, hashing,
character classification) instead of indexing into a literal alphabet
string.

### Examples
```
"hi".to_code_points()    // [104, 105]
"é".to_code_points()     // [233]
"🐑".to_code_points()    // [128017]
"".to_code_points()      // []
```

### to_lower_case

```baml
function to_lower_case(self: string) -> string
```

Returns the string with all Unicode characters converted to lowercase.

### to_upper_case

```baml
function to_upper_case(self: string) -> string
```

Returns the string with all Unicode characters converted to uppercase.

### to_utf8

```baml
function to_utf8(self: string) -> uint8array
```

Returns the string encoded as a `uint8array` of UTF-8 bytes.

The resulting byte array has length `self.byte_length()`. Inverse of the
static `string.from_utf8(bytes)`.

### Examples
```
"hi".to_utf8()     // [0x68, 0x69]
"é".to_utf8()      // [0xC3, 0xA9]
"".to_utf8()       // []
```

### trim

```baml
function trim(self: string) -> string
```

Returns the string with leading and trailing whitespace removed.

Whitespace is defined per Unicode `White_Space` and includes ASCII spaces,
tabs, newlines, and carriage returns as well as Unicode whitespace.

### Examples
```
"  hi  ".trim()    // "hi"
"\n\thi\n".trim()  // "hi"
"hi".trim()        // "hi"
```

### trim_end

```baml
function trim_end(self: string) -> string
```

Returns the string with trailing whitespace removed (leading whitespace
is preserved). Whitespace follows Unicode `White_Space`.

### Examples
```
"  hi  ".trim_end()  // "  hi"
"hi\n".trim_end()    // "hi"
```

### trim_start

```baml
function trim_start(self: string) -> string
```

Returns the string with leading whitespace removed (trailing whitespace
is preserved). Whitespace follows Unicode `White_Space`.

### Examples
```
"  hi  ".trim_start()  // "hi  "
"\nhi".trim_start()    // "hi"
```

_Source: `<builtin>/baml/string.baml:200`_
