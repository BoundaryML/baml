/// A UTF-8 encoded string.
///
/// Strings are immutable: every method here returns a new string rather than
/// modifying `self`. Quoted strings may span multiple lines; backtick strings
/// interpolate.
///
/// Positions are **character** (Unicode code point) offsets, never byte offsets.
/// `length`, `at`, `code_point_at`, `slice`, `index_of`, and `last_index_of` all
/// agree with each other, and all disagree with `byte_length` as soon as one
/// character takes more than one UTF-8 byte. A method taking a position also
/// accepts a negative one, counting from the end. Python's `str` indexes the same
/// way; JavaScript indexes by UTF-16 unit and so can cut an emoji in half.
///
/// The `is_*` predicates ask whether **every** character has the property, so all
/// of them are true on the empty string. The `is_ascii_*` family is false for any
/// character at or above U+0080.
class String {
    /// Renders any value as human-readable text.
    ///
    /// Dispatches to `baml.ToString.to_string` when `value`'s runtime type
    /// implements it, and to a default structural rendering otherwise.
    ///
    /// # Examples
    ///
    /// ```baml
    /// string.from(42)            // "42"
    /// string.from(true)          // "true"
    /// string.from([1, 2, 3])     // "[1, 2, 3]"
    /// ```
    function from<T>(value: T) -> string throws never {
        match (value) {
            baml.ToString => value.to_string(),
            _ => root._to_string_default(value),
        }
    }

    /// Returns the number of characters (Unicode code points).
    ///
    /// `O(1)`: the count is cached at construction time. It equals `byte_length()`
    /// for ASCII text and is smaller for anything with a multi-byte character.
    ///
    /// Python's `len(s)` counts the same way. JavaScript's
    /// [`length`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/length)
    /// does not: it counts UTF-16 units, so `"😀"` is 2 there and 1 here.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".length()  // 5
    /// "héllo".length()  // 5
    /// "😀".length()     // 1
    /// "".length()       // 0
    /// ```
    function length(self) -> int throws never {
        self.char_count()
    }

    /// Alias for `length()`.
    function char_count(self) -> int throws never {
        $rust_function
    }

    /// Returns true if the string has no characters.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "".is_empty()       // true
    /// "hello".is_empty()  // false
    /// ```
    function is_empty(self) -> bool throws never {
        self.byte_length() == 0
    }

    /// Returns the length in UTF-8 bytes.
    ///
    /// `O(1)`. This is the size on the wire — the length of `to_utf8()` — and so
    /// the number serialization, network I/O, and buffer allocation want. Python's
    /// `len(s.encode())`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".byte_length()  // 5
    /// "héllo".byte_length()  // 6
    /// "😀".byte_length()     // 4
    /// ```
    function byte_length(self) -> int throws never {
        $rust_function
    }

    /// Returns the lowercase equivalent of this string.
    ///
    /// Full Unicode case conversion, and not locale-aware: the Turkish/Azeri
    /// dotless-i rules are not applied. Conversion is context-sensitive, so a
    /// character's lowercase form can depend on its neighbors.
    ///
    /// Python's [`str.lower`](https://docs.python.org/3/library/stdtypes.html#str.lower),
    /// JavaScript's [`toLowerCase`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/toLowerCase).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "HELLO WORLD".to_lower_case()  // "hello world"
    /// "ὈΔΥΣΣΕΎΣ".to_lower_case()     // "ὀδυσσεύς" (final Σ lowercases to ς, not σ)
    /// ```
    function to_lower_case(self) -> string throws never {
        $rust_function
    }

    /// Returns the uppercase equivalent of this string.
    ///
    /// Full Unicode case conversion, and not locale-aware: the Turkish/Azeri
    /// dotless-i rules are not applied. Conversion is one-to-many, so the result
    /// can be longer than `self`.
    ///
    /// Python's [`str.upper`](https://docs.python.org/3/library/stdtypes.html#str.upper),
    /// JavaScript's [`toUpperCase`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/toUpperCase).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".to_upper_case()   // "HELLO"
    /// "tschüß".to_upper_case()  // "TSCHÜSS" (ß expands to SS)
    /// ```
    function to_upper_case(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with leading and trailing whitespace removed.
    ///
    /// Whitespace is the Unicode `White_Space` property: ASCII spaces, tabs,
    /// newlines, and carriage returns, plus non-ASCII whitespace.
    ///
    /// Python's [`str.strip`](https://docs.python.org/3/library/stdtypes.html#str.strip),
    /// JavaScript's [`trim`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/trim).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "  hi  ".trim()    // "hi"
    /// "\n\thi\n".trim()  // "hi"
    /// "hi".trim()        // "hi"
    /// ```
    function trim(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with leading whitespace removed; trailing whitespace is
    /// kept. Whitespace is defined as in `trim`.
    ///
    /// Python's [`str.lstrip`](https://docs.python.org/3/library/stdtypes.html#str.lstrip),
    /// JavaScript's [`trimStart`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/trimStart).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "  hi  ".trim_start()  // "hi  "
    /// "\nhi".trim_start()    // "hi"
    /// ```
    function trim_start(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with trailing whitespace removed; leading whitespace is
    /// kept. Whitespace is defined as in `trim`.
    ///
    /// Python's [`str.rstrip`](https://docs.python.org/3/library/stdtypes.html#str.rstrip),
    /// JavaScript's [`trimEnd`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/trimEnd).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "  hi  ".trim_end()  // "  hi"
    /// "hi\n".trim_end()    // "hi"
    /// ```
    function trim_end(self) -> string throws never {
        $rust_function
    }

    /// Returns true if `search` occurs anywhere in the string.
    ///
    /// Python's `search in self`, JavaScript's
    /// [`includes`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/includes).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".includes("ll")  // true
    /// "hello".includes("world")  // false
    /// ```
    function includes(self, search: string) -> bool throws never {
        $rust_function
    }

    /// Returns true if the string starts with `prefix`.
    ///
    /// Python's [`str.startswith`](https://docs.python.org/3/library/stdtypes.html#str.startswith),
    /// JavaScript's [`startsWith`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/startsWith).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".starts_with("h")  // true
    /// "hello".starts_with("world")  // false
    /// ```
    function starts_with(self, prefix: string) -> bool throws never {
        $rust_function
    }

    /// Returns true if the string ends with `suffix`.
    ///
    /// Python's [`str.endswith`](https://docs.python.org/3/library/stdtypes.html#str.endswith),
    /// JavaScript's [`endsWith`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/endsWith).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".ends_with("o")  // true
    /// "hello".ends_with("world")  // false
    /// ```
    function ends_with(self, suffix: string) -> bool throws never {
        $rust_function
    }

    /// Splits the string on `delimiter`.
    ///
    /// An empty `delimiter` splits into single characters. A `delimiter` that never
    /// occurs yields a one-element array holding the whole string.
    ///
    /// JavaScript's [`split`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/split),
    /// except that splitting on `""` yields whole characters rather than UTF-16
    /// units. Python's `str.split` rejects an empty separator outright.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "a,b,c".split(",")     // ["a", "b", "c"]
    /// "hello".split("")      // ["h", "e", "l", "l", "o"]
    /// "no match".split(",")  // ["no match"]
    /// ```
    function split(self, delimiter: string) -> string[] throws never {
        $rust_function
    }

    /// Returns each Unicode code point as a one-character string.
    ///
    /// Python's `list(s)`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".chars()  // ["h", "e", "l", "l", "o"]
    /// "é😀".chars()    // ["é", "😀"]
    /// "".chars()       // []
    /// ```
    function chars(self) -> string[] throws never {
        $rust_function
    }

    /// Splits the string into lines, on either `\n` or `\r\n`.
    ///
    /// The terminator is not part of the returned strings, and a final terminator
    /// does not produce a trailing empty string. A lone `\r` is not a terminator.
    ///
    /// Narrower than Python's
    /// [`str.splitlines`](https://docs.python.org/3/library/stdtypes.html#str.splitlines),
    /// which also breaks on form feed, `\v`, and U+2028.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "a\nb\nc".lines()    // ["a", "b", "c"]
    /// "a\nb\n".lines()     // ["a", "b"]      (no trailing empty)
    /// "a\r\nb".lines()     // ["a", "b"]      (CRLF handled)
    /// "".lines()           // []
    /// "\n".lines()         // [""]
    /// ```
    function lines(self) -> string[] throws never {
        $rust_function
    }

    /// Returns the characters in `[start, end)`.
    ///
    /// Both bounds are character offsets, matching `length()`, and may be negative
    /// to count from the end. Resolved offsets are clamped to `[0, length()]`, and
    /// an `end` at or before `start` yields `""`.
    ///
    /// JavaScript's [`slice`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/slice)
    /// clamps and handles negatives the same way, but indexes by UTF-16 unit.
    /// Python's `s[start:end]`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello world".slice(0, 5)   // "hello"
    /// "hello".slice(2, 100)       // "llo" (end clamped)
    /// "hello".slice(-3, -1)       // "ll" (counts from the end)
    /// "😀hello".slice(0, 1)       // "😀"
    /// "😀hello".slice(1, 4)       // "hel"
    /// ```
    function slice(self, start: int, end: int) -> string throws never {
        $rust_function
    }

    /// Replaces the first occurrence of `search` with `replacement`.
    ///
    /// JavaScript's [`replace`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/replace)
    /// given a string pattern. Python's `str.replace` replaces every occurrence
    /// unless given a count, so this is `s.replace(search, replacement, 1)`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello world".replace("world", "universe")  // "hello universe"
    /// ```
    function replace(self, search: string, replacement: string) -> string throws never {
        $rust_function
    }

    /// Returns the character index of the first occurrence of `search`, or `null`
    /// if it does not occur.
    ///
    /// Python's [`str.find`](https://docs.python.org/3/library/stdtypes.html#str.find)
    /// and JavaScript's
    /// [`indexOf`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/indexOf)
    /// signal absence with `-1` instead. The index counts characters, as Python's
    /// does; JavaScript's counts UTF-16 units.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello world".index_of("world")  // 6
    /// "héllo".index_of("l")            // 2
    /// "😀hello".index_of("h")          // 1
    /// "abc".index_of("z")              // null
    /// ```
    function index_of(self, search: string) -> int? throws never {
        $rust_function
    }

    /// Returns the character index at which the last occurrence of `search` starts,
    /// or `null` if it does not occur.
    ///
    /// Python's [`str.rfind`](https://docs.python.org/3/library/stdtypes.html#str.rfind),
    /// JavaScript's [`lastIndexOf`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/lastIndexOf);
    /// both signal absence with `-1`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello world".last_index_of("world")  // 6
    /// "héllo".last_index_of("l")            // 3 (the second "l"; `index_of` finds the first, at 2)
    /// "😀hello".last_index_of("h")          // 1
    /// "abc".last_index_of("z")              // null
    /// ```
    function last_index_of(self, search: string) -> int? throws never {
        $rust_function
    }

    /// Returns the character at `index` as a one-character string, or `null` if
    /// `index` is out of range.
    ///
    /// `index` is a character offset; a negative one counts from the end, so `-1`
    /// is the last character.
    ///
    /// JavaScript's [`at`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/at)
    /// returns one UTF-16 unit, which can be half an emoji; this returns a whole
    /// character.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".at(0)   // "h"
    /// "héllo".at(1)   // "é"
    /// "😀hello".at(0) // "😀"
    /// "hello".at(-1)  // "o"
    /// "hi".at(99)     // null
    /// ```
    function at(self, index: int) -> string? throws never {
        $rust_function
    }

    /// Returns the Unicode code point of the character at `index`, as an `int`.
    ///
    /// The numeric counterpart of `at`: same character indexing, a negative
    /// `index` counting from the end, and `null` when out of range. The value is
    /// always in `[0, 0x10FFFF]`, so it round-trips through
    /// `string.from_code_points([cp])`.
    ///
    /// Python's `ord(s[i])`. JavaScript's
    /// [`codePointAt`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/codePointAt)
    /// takes a UTF-16 offset, which drifts from this one after any character
    /// outside the BMP.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "A".code_point_at(0)        // 65
    /// "é".code_point_at(0)        // 233
    /// "🐑".code_point_at(0)       // 128017
    /// "hello".code_point_at(-1)   // 111 ("o")
    /// "😀hello".code_point_at(1)  // 104 ("h", not a UTF-16 unit)
    /// "hi".code_point_at(99)      // null
    /// ```
    function code_point_at(self, index: int) -> int? throws never {
        $rust_function
    }

    /// Returns `self` repeated `count` times. A negative `count` counts as 0.
    ///
    /// Python's `s * count` also yields `""` for a negative count; JavaScript's
    /// [`repeat`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/repeat)
    /// throws a `RangeError`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "ab".repeat(3)   // "ababab"
    /// "hi".repeat(0)   // ""
    /// "hi".repeat(-1)  // ""
    /// ```
    function repeat(self, count: int) -> string throws never {
        $rust_function
    }

    /// Replaces every occurrence of `search` with `replacement`.
    ///
    /// Python's [`str.replace`](https://docs.python.org/3/library/stdtypes.html#str.replace),
    /// JavaScript's [`replaceAll`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/replaceAll).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello world".replace_all("l", "L")  // "heLLo worLd"
    /// ```
    function replace_all(self, search: string, replacement: string) -> string throws never {
        $rust_function
    }

    // ─── Unicode-aware character-class predicates ─────────────────────────────
    //
    // Every `is_*` predicate below tests that **all** characters in the string
    // satisfy the predicate. The empty string returns `true` (the universal
    // quantifier over an empty set is vacuously true). For single-character
    // strings these behave like character-class checks.

    /// Returns true if every character is numeric per Unicode: general categories
    /// `Nd`, `Nl`, and `No` — decimal digits, letter-numbers, and other-numbers.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "12345".is_numeric()  // true
    /// "Ⅷ".is_numeric()      // true (Roman numeral, Nl)
    /// "12a".is_numeric()    // false
    /// "".is_numeric()       // true (vacuous)
    /// ```
    function is_numeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character has the Unicode `Alphabetic` property.
    ///
    /// Broader than general category `L` (letters): `Alphabetic` also covers
    /// letter-numbers like Roman numerals (category `Nl`) and a handful of
    /// combining marks carrying the derived `Other_Alphabetic` property. Python's
    /// [`str.isalpha`](https://docs.python.org/3/library/stdtypes.html#str.isalpha)
    /// tests `L` alone, and so rejects the `Ⅷ` this accepts.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "héllo".is_alphabetic()  // true
    /// "漢字".is_alphabetic()    // true
    /// "abc1".is_alphabetic()   // false
    /// "Ⅷ".is_alphabetic()      // true (Roman numeral, category `Nl` but still `Alphabetic`)
    /// ```
    function is_alphabetic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is alphabetic or numeric per Unicode.
    ///
    /// The test is per character, so `"a1"` passes even though neither
    /// `is_alphabetic` nor `is_numeric` holds for the whole string.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "abc123".is_alphanumeric()  // true
    /// "héllo7".is_alphanumeric()  // true
    /// "a b".is_alphanumeric()     // false (space is not)
    /// ```
    function is_alphanumeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is uppercase per Unicode: general category
    /// `Lu`, plus other characters with the `Uppercase` property.
    ///
    /// Digits and most symbols are neither upper- nor lowercase, so a string
    /// containing one fails both this and `is_lowercase`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "HELLO".is_uppercase()  // true
    /// "Hello".is_uppercase()  // false
    /// "123".is_uppercase()    // false (digits aren't uppercase)
    /// ```
    function is_uppercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is lowercase per Unicode: general category
    /// `Ll`, plus other characters with the `Lowercase` property.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".is_lowercase()  // true
    /// "Hello".is_lowercase()  // false
    /// ```
    function is_lowercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character has the Unicode `White_Space` property:
    /// space, tab, newline, NBSP, and other Unicode whitespace.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "   ".is_whitespace()      // true
    /// " \t\n".is_whitespace()    // true
    /// "a b".is_whitespace()      // false
    /// ```
    function is_whitespace(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is a control character per Unicode:
    /// general category `Cc`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "\n\t".is_control()  // true
    /// "a".is_control()     // false
    /// ```
    function is_control(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is graphic: neither a control character
    /// nor whitespace.
    ///
    /// Unicode has no single `Graphic` property. This is a convenience predicate
    /// for "visible/printing", built from the Unicode notions of control and
    /// whitespace.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "abc".is_graphic()      // true
    /// "héllo".is_graphic()    // true
    /// "a b".is_graphic()      // false (space is not graphic)
    /// "a\n".is_graphic()      // false (newline is control)
    /// ```
    function is_graphic(self) -> bool throws never {
        $rust_function
    }

    // ─── ASCII-only predicates ────────────────────────────────────────────────
    //
    // Each `is_ascii_*` returns false on any non-ASCII character (≥ U+0080).
    // Empty string returns true.

    /// Returns true if every character is ASCII: a code point in `[0x00, 0x7F]`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".is_ascii()    // true
    /// "héllo".is_ascii()    // false (`é` is U+00E9)
    /// ```
    function is_ascii(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII decimal digit, `0` through `9`.
    ///
    /// Stricter than `is_numeric`, which accepts non-ASCII numerals.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "12345".is_ascii_numeric()  // true
    /// "Ⅷ".is_ascii_numeric()      // false (Roman numeral)
    /// ```
    function is_ascii_numeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII letter, `A` through `Z` or `a`
    /// through `z`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".is_ascii_alphabetic()  // true
    /// "héllo".is_ascii_alphabetic()  // false
    /// ```
    function is_ascii_alphabetic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII letter or digit.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "abc123".is_ascii_alphanumeric()  // true
    /// "héllo".is_ascii_alphanumeric()   // false
    /// ```
    function is_ascii_alphanumeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII uppercase letter, `A` through
    /// `Z`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "HELLO".is_ascii_uppercase()  // true
    /// "héllo".is_ascii_uppercase()  // false
    /// ```
    function is_ascii_uppercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII lowercase letter, `a` through
    /// `z`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".is_ascii_lowercase()  // true
    /// "héllo".is_ascii_lowercase()  // false
    /// ```
    function is_ascii_lowercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is ASCII whitespace: space (`\x20`),
    /// horizontal tab (`\t`), line feed (`\n`), form feed (`\x0C`), or carriage
    /// return (`\r`).
    ///
    /// Narrower than `is_whitespace`, which accepts Unicode whitespace like NBSP.
    ///
    /// # Examples
    ///
    /// ```baml
    /// " ".is_ascii_whitespace()  // true
    /// "héllo".is_ascii_whitespace()  // false
    /// ```
    function is_ascii_whitespace(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII control character: a code point
    /// in `[0x00, 0x1F]`, or DEL (`0x7F`).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "".is_ascii_control()  // true
    /// "héllo".is_ascii_control()  // false
    /// ```
    function is_ascii_control(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII graphic character: printable,
    /// non-space, non-control, meaning a code point in `[0x21, 0x7E]`.
    ///
    /// The ASCII space is not graphic.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "".is_ascii_graphic()  // true
    /// "héllo".is_ascii_graphic()  // false
    /// ```
    function is_ascii_graphic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII hexadecimal digit: `0` through
    /// `9`, `a` through `f`, or `A` through `F`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "deadBEEF".is_ascii_hex()  // true
    /// "0x123".is_ascii_hex()     // false (`x` is not a hex digit)
    /// ```
    function is_ascii_hex(self) -> bool throws never {
        $rust_function
    }

    /// Returns the string encoded as UTF-8 bytes.
    ///
    /// The result has length `self.byte_length()`. Inverse of `string.from_utf8`.
    ///
    /// Python's [`str.encode`](https://docs.python.org/3/library/stdtypes.html#str.encode),
    /// JavaScript's [`TextEncoder.encode`](https://developer.mozilla.org/en-US/docs/Web/API/TextEncoder/encode).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hi".to_utf8()     // [0x68, 0x69]
    /// "é".to_utf8()      // [0xC3, 0xA9]
    /// "".to_utf8()       // []
    /// ```
    function to_utf8(self) -> uint8array throws never {
        $rust_function
    }

    /// Decodes UTF-8 bytes into a string, rejecting anything malformed.
    ///
    /// For lossy decoding, which substitutes U+FFFD for each invalid sequence,
    /// use `uint8array.to_string()`.
    ///
    /// Python's [`bytes.decode`](https://docs.python.org/3/library/stdtypes.html#bytes.decode),
    /// JavaScript's [`TextDecoder`](https://developer.mozilla.org/en-US/docs/Web/API/TextDecoder)
    /// with `fatal: true`.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `utf8` is not valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```baml
    /// string.from_utf8(b"\x68\x69")          // "hi"
    /// string.from_utf8(b"\xC3\xA9")          // "é"
    /// string.from_utf8(b"\xFF")              // throws — 0xFF is invalid UTF-8
    /// ```
    function from_utf8(utf8: uint8array) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns the string's Unicode code points as `int`s.
    ///
    /// One element per character, so `self.length()` of them, each in
    /// `[0, 0x10FFFF]`. The exact inverse of `string.from_code_points`:
    /// `string.from_code_points(s.to_code_points())` equals `s` for every `s`.
    ///
    /// Reach for this whenever you need characters as numbers — checksums, base-N
    /// encoding, hashing, classification — rather than indexing into a literal
    /// alphabet string. Python's `[ord(c) for c in s]`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hi".to_code_points()    // [104, 105]
    /// "é".to_code_points()     // [233]
    /// "🐑".to_code_points()    // [128017]
    /// "".to_code_points()      // []
    /// ```
    function to_code_points(self) -> int[] throws never {
        $rust_function
    }

    /// Builds a string from an array of Unicode code points.
    ///
    /// Each value must be in `[0, 0x10FFFF]` and must not be a UTF-16 surrogate,
    /// the range `[0xD800, 0xDFFF]`. Python's `chr` and JavaScript's
    /// [`String.fromCodePoint`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/String/fromCodePoint)
    /// both accept a lone surrogate; this does not.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` on any invalid value, identifying the
    ///   offending position in the array.
    ///
    /// # Examples
    ///
    /// ```baml
    /// string.from_code_points([104, 105])         // "hi"
    /// string.from_code_points([233])              // "é"
    /// string.from_code_points([128017])           // "🐑"
    /// string.from_code_points([-1])               // throws — out of range
    /// string.from_code_points([55296])            // throws — surrogate (U+D800)
    /// ```
    function from_code_points(unicode: int[]) -> string throws root.errors.InvalidArgument {
        $rust_function
    }
}

// `Rhs` is `string`, not a generic parameter: an `implement<T> Add<T> for string`
// would put `T` only in the interface args, and neither dispatch path can bind it
// there. The runtime resolver infers an impl's generics by matching its *for*
// pattern against the receiver (`resolve_implements_rule`), so `T` stays unbound
// and a `T extends Add<T>` call site dies with an unresolved virtual call; the
// direct path has no `string + int` opcode and aborts with `cannot apply binary
// operation`. Widening `+` to render any right operand needs both of those
// fixed first.
implement root.ops.Add<string> for string {
    type Output = string

    /// Concatenates `self` and `rhs`. Backs the `+` operator on strings.
    ///
    /// # Examples
    ///
    /// ```baml
    /// "foo" + "bar"  // "foobar"
    /// ```
    function add(self, rhs: string) -> string throws never {
        `${self}${rhs}`
    }
}
