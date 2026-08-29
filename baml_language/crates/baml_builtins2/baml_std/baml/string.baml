/// A UTF-8 encoded string.
///
/// Quoted strings may span multiple lines. Backtick strings support interpolation.
/// All methods returning a new string do not mutate `self` unless noted otherwise.
class String {
    /// Renders any value as a human-readable string.
    ///
    /// If `value`'s runtime type implements `baml.ToString`, its `to_string`
    /// override is used; otherwise a default structural rendering is produced.
    /// Never throws.
    ///
    /// ```
    /// string.from(42)            // "42"
    /// string.from(true)          // "true"
    /// string.from([1, 2, 3])     // "[1, 2, 3]"
    /// ```
    ///
    /// Ideally this would read:
    ///
    /// ```
    /// if (value is baml.ToString) { value.to_string() }
    /// else { root._to_string_default(value) }
    /// ```
    ///
    /// but that does not work from the stdlib today. `is <interface>` and
    /// interface method dispatch are resolved at *compile time* by expanding the
    /// interface to its known implementor classes, and that implementor set is
    /// per-package: it is built from the package's dependencies + itself, never
    /// its dependents. `string.from` lives in the `baml` package, which a user's
    /// `implements baml.ToString` sits *above*, so from here the implementor set
    /// for `baml.ToString` is empty — the `is` test folds to constant `false` and
    /// the override never dispatches. (Same boundary `Sortable.sort`'s
    /// `_compare_shim` and `baml.json.to_json` work around.)
    ///
    /// So dispatch is resolved on `value`'s *runtime* class via
    /// `baml._to_string_shim` instead. TODO: once Kai's TIR rework lands (it moves
    /// interface conformance checks to runtime), replace this with the literal
    /// `is baml.ToString` form above and drop `_to_string_shim`.
    function from<T>(value: T) -> string throws never {
        root._to_string_shim(value)
    }

    /// Returns the number of **Unicode code points** (characters) in the string.
    ///
    /// This is `O(1)` — the count is cached at construction time. For ASCII
    /// text it equals `byte_length()`, but for strings with multi-byte UTF-8
    /// characters they differ.
    ///
    /// # Examples
    /// ```
    /// "hello".length()  // 5
    /// "héllo".length()  // 5
    /// "😀".length()     // 1
    /// "".length()       // 0
    /// ```
    function length(self) -> int throws never {
        $rust_function
    }

    /// Alias for `length()`.
    function char_count(self) -> int throws never {
        $rust_function
    }

    /// Returns true if the string has no characters.
    ///
    /// # Examples
    /// ```
    /// "".is_empty()       // true
    /// "hello".is_empty()  // false
    /// ```
    function is_empty(self) -> bool throws never {
        self.byte_length() == 0
    }

    /// Returns the length of the string in **UTF-8 bytes**.
    ///
    /// This is `O(1)`. Useful for serialization, network I/O, and buffer
    /// allocation where byte size matters.
    ///
    /// # Examples
    /// ```
    /// "hello".byte_length()  // 5
    /// "héllo".byte_length()  // 6
    /// "😀".byte_length()     // 4
    /// ```
    function byte_length(self) -> int throws never {
        $rust_function
    }

    /// Returns the string with all Unicode characters converted to lowercase.
    function to_lower_case(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with all Unicode characters converted to uppercase.
    function to_upper_case(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with leading and trailing whitespace removed.
    ///
    /// Whitespace is defined per Unicode `White_Space` and includes ASCII spaces,
    /// tabs, newlines, and carriage returns as well as Unicode whitespace.
    ///
    /// # Examples
    /// ```
    /// "  hi  ".trim()    // "hi"
    /// "\n\thi\n".trim()  // "hi"
    /// "hi".trim()        // "hi"
    /// ```
    function trim(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with leading whitespace removed (trailing whitespace
    /// is preserved). Whitespace follows Unicode `White_Space`.
    ///
    /// # Examples
    /// ```
    /// "  hi  ".trim_start()  // "hi  "
    /// "\nhi".trim_start()    // "hi"
    /// ```
    function trim_start(self) -> string throws never {
        $rust_function
    }

    /// Returns the string with trailing whitespace removed (leading whitespace
    /// is preserved). Whitespace follows Unicode `White_Space`.
    ///
    /// # Examples
    /// ```
    /// "  hi  ".trim_end()  // "  hi"
    /// "hi\n".trim_end()    // "hi"
    /// ```
    function trim_end(self) -> string throws never {
        $rust_function
    }

    /// Returns true if `search` appears anywhere in the string.
    function includes(self, search: string) -> bool throws never {
        $rust_function
    }

    /// Returns true if the string starts with `prefix`.
    function starts_with(self, prefix: string) -> bool throws never {
        $rust_function
    }

    /// Returns true if the string ends with `suffix`.
    function ends_with(self, suffix: string) -> bool throws never {
        $rust_function
    }

    /// Splits the string by `delimiter` and returns an array of substrings.
    ///
    /// # Examples
    /// ```
    /// "a,b,c".split(",")     // ["a", "b", "c"]
    /// "hello".split("")      // ["h", "e", "l", "l", "o"]
    /// "no match".split(",")  // ["no match"]
    /// ```
    function split(self, delimiter: string) -> string[] throws never {
        $rust_function
    }

    /// Returns the string's Unicode code points as one-character strings.
    ///
    /// # Examples
    /// ```
    /// "hello".chars()  // ["h", "e", "l", "l", "o"]
    /// "é😀".chars()    // ["é", "😀"]
    /// "".chars()       // []
    /// ```
    function chars(self) -> string[] throws never {
        $rust_function
    }

    /// Splits the string into lines, recognizing both `\n` and `\r\n` as line
    /// terminators. The terminator is **not** included in the returned strings.
    /// A final terminator does not produce a trailing empty string.
    ///
    /// # Examples
    /// ```
    /// "a\nb\nc".lines()    // ["a", "b", "c"]
    /// "a\nb\n".lines()     // ["a", "b"]      (no trailing empty)
    /// "a\r\nb".lines()     // ["a", "b"]      (CRLF handled)
    /// "".lines()           // []
    /// "\n".lines()         // [""]
    /// ```
    function lines(self) -> string[] throws never {
        $rust_function
    }

    /// Returns the substring between two **character** offsets `[start, end)`.
    ///
    /// Both `start` and `end` are codepoint indices, matching `length()`. Negative
    /// indices count from the end. The resolved offsets are clamped to
    /// `[0, length()]`, and an `end` that resolves at or before `start` yields an
    /// empty string. Never throws.
    ///
    /// # Examples
    /// ```
    /// "hello world".slice(0, 5)   // "hello"
    /// "hello".slice(2, 100)       // "llo" (end clamped)
    /// "hello".slice(-3, -1)       // "ll" (counts from the end)
    /// "😀hello".slice(0, 1)       // "😀"
    /// "😀hello".slice(1, 4)       // "hel"
    /// ```
    function slice(self, start: int, end: int) -> string throws never {
        $rust_function
    }

    /// Replaces the first occurrence of `search`.
    ///
    /// `search` is either a `string`, matched literally, or a
    /// `baml.regex.Regex`, matched as a pattern. `replacement` is either a
    /// `string` or a callback receiving the `baml.regex.Match`.
    ///
    /// With a `baml.regex.Regex` search, a `string` replacement is a template:
    /// `$0` is the whole match, `$1`..`$n` the numbered groups, `$name` and
    /// `${name}` the named ones, and `$$` a literal `$`. A group that did not
    /// participate — or does not exist — expands to nothing. `$1abc` names a
    /// group `1abc`; write `${1}abc` to delimit group `1`.
    ///
    /// With a literal `string` search, a `string` replacement stays literal:
    /// `$1` in existing code keeps meaning `$1`.
    ///
    /// A callback's return value is inserted verbatim, so case adaptation,
    /// table lookup, and macro expansion need no separate method. Under a
    /// literal search the `Match` it receives has only the whole match in
    /// `groups` and an empty `named`.
    ///
    /// # Examples
    /// ```
    /// "a.b".replace(".", "-")                                  // "a-b"
    /// "a.b".replace(baml.regex.compile("."), "-")              // "-.b"
    /// "5 cm".replace(baml.regex.compile("(\\d+) cm"), "${1}mm") // "5mm"
    /// ```
    function replace<E>(
        self,
        search: string | root.regex.Regex,
        replacement: string | ((root.regex.Match) -> string throws E),
    ) -> string throws E {
        self._replace_impl(search, replacement, false)
    }

    /// Returns the **character index** of the first occurrence of `search`, or
    /// `null` if not found.
    ///
    /// # Examples
    /// ```
    /// "hello world".index_of("world")  // 6
    /// "héllo".index_of("l")            // 2
    /// "😀hello".index_of("h")          // 1
    /// "abc".index_of("z")              // null
    /// ```
    function index_of(self, search: string) -> int? throws never {
        $rust_function
    }

    /// Returns the **character index** of the start of the last occurrence of `search`, or
    /// `null` if not found.
    ///
    /// # Examples
    /// ```
    /// "hello world".last_index_of("world")  // 6
    /// "héllo".last_index_of("l")            // 2
    /// "😀hello".last_index_of("h")          // 1
    /// "abc".last_index_of("z")              // null
    /// ```
    function last_index_of(self, search: string) -> int? throws never {
        $rust_function
    }

    /// Returns the character at the given **codepoint index** as a
    /// one-character string.
    ///
    /// Negative indices count from the end: `-1` is the last character.
    /// Returns `null` if the index is out of range.
    ///
    /// # Examples
    /// ```
    /// "hello".at(0)   // "h"
    /// "héllo".at(1)   // "é"
    /// "😀hello".at(0) // "😀"
    /// "hello".at(-1)  // "o"
    /// "hi".at(99)     // null
    /// ```
    function at(self, index: int) -> string? throws never {
        $rust_function
    }

    /// Returns the **Unicode code point** of the character at the given codepoint
    /// index, as an `int`.
    ///
    /// This is the numeric counterpart of `at`, which returns the character as a
    /// one-character string. Negative indices count from the end: `-1` is the last
    /// character. Returns `null` if the index is out of range.
    ///
    /// The returned value is always in `[0, 0x10FFFF]`, so it round-trips through
    /// `string.from_code_points([cp])`.
    ///
    /// # Examples
    /// ```
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

    /// Returns a new string that repeats `self` the given number of times.
    /// Negative counts are treated as 0.
    ///
    /// # Examples
    /// ```
    /// "ab".repeat(3)   // "ababab"
    /// "hi".repeat(0)   // ""
    /// "hi".repeat(-1)  // ""
    /// ```
    function repeat(self, count: int) -> string throws never {
        $rust_function
    }

    /// Replaces every occurrence of `search`.
    ///
    /// Same search and replacement rules as `replace`, applied to every
    /// non-overlapping match instead of only the first. With a pattern that
    /// can match the empty string the scan still terminates: it advances one
    /// codepoint past an empty match.
    ///
    /// # Examples
    /// ```
    /// "a.b.c".replace_all(".", "-")                       // "a-b-c"
    /// text.replace_all(baml.regex.word(term, ignore_case = true), adapt)
    /// ```
    function replace_all<E>(
        self,
        search: string | root.regex.Regex,
        replacement: string | ((root.regex.Match) -> string throws E),
    ) -> string throws E {
        self._replace_impl(search, replacement, true)
    }

    // ─── Replacement plumbing ─────────────────────────────────────────────────
    //
    // `replace` and `replace_all` differ only in a `bool`, so the four-way
    // search × replacement dispatch lives in one place. Each leaf is the
    // cheapest form of the job: two literal-only native calls, two engine-side
    // template expansions, and two BAML scan loops for the callback forms,
    // which are the only ones that have to re-enter user code per match.

    /// Shared body of `replace` (`all = false`) and `replace_all`.
    function _replace_impl<E>(
        self,
        search: string | root.regex.Regex,
        replacement: string | ((root.regex.Match) -> string throws E),
        all: bool,
    ) -> string throws E {
        match (search) {
            string => match (replacement) {
                string => if (all) {
                    self._replace_all_literal(search, replacement)
                } else {
                    self._replace_literal(search, replacement)
                },
                let f: (root.regex.Match) -> string throws E => {
                    self._replace_literal_with(search, f, all)
                },
            },
            root.regex.Regex => match (replacement) {
                string => if (all) {
                    search._replace_all_template(self._as_string(), replacement)
                } else {
                    search._replace_template(self._as_string(), replacement)
                },
                let f: (root.regex.Match) -> string throws E => {
                    self._replace_pattern_with(search, f, all)
                },
            },
        }
    }

    /// Literal `search`, replacement computed per occurrence by `f`.
    function _replace_literal_with<E>(
        self,
        search: string,
        f: (root.regex.Match) -> string throws E,
        all: bool,
    ) -> string throws E {
        let length = self.length();
        let width = search.length();
        let pieces: string[] = [];
        let pos = 0;
        let scanning = true;
        while (scanning && pos <= length) {
            match (self.slice(pos, length).index_of(search)) {
                null => {
                    scanning = false;
                },
                let offset: int => {
                    let at = pos + offset;
                    pieces.push(self.slice(pos, at));
                    pieces.push(f(root.regex._literal_match(search, at)));
                    // An empty `search` matches between every pair of
                    // characters. Stepping one codepoint past it terminates the
                    // scan and carries the stepped-over character through; the
                    // `slice` is empty when the match was at the very end.
                    pieces.push(if (width == 0) { self.slice(at, at + 1) } else { "" });
                    pos = if (width == 0) { at + 1 } else { at + width };
                    scanning = all;
                },
            }
        }
        pieces.push(self.slice(pos, length));
        pieces.join("")
    }

    /// Pattern `search`, replacement computed per match by `f`.
    function _replace_pattern_with<E>(
        self,
        search: root.regex.Regex,
        f: (root.regex.Match) -> string throws E,
        all: bool,
    ) -> string throws E {
        let text = self._as_string();
        let matches: root.regex.Match[] = if (all) {
            search.match_all(text)
        } else {
            match (search.match(text)) {
                null => [],
                let first: root.regex.Match => [first],
            }
        };
        let pieces: string[] = [];
        let pos = 0;
        for (let m in matches) {
            pieces.push(self.slice(pos, m.start));
            pieces.push(f(m));
            pos = m.end;
        }
        pieces.push(self.slice(pos, self.length()));
        pieces.join("")
    }

    /// `self` as the `string` primitive.
    ///
    /// Inside a `class String` body `self` is the class, not the primitive, so
    /// it cannot be handed to an API declared over `string` — `baml.regex`'s
    /// methods, here. A whole-length `slice` is the cheap way across:
    /// `substring` hands back a view of the same bytes, where interpolation
    /// would route through `string.from`'s override-honoring walker and copy.
    function _as_string(self) -> string throws never {
        self.slice(0, self.length())
    }

    /// `replace` restricted to a literal search and a literal replacement.
    function _replace_literal(self, search: string, replacement: string) -> string throws never {
        $rust_function
    }

    /// `replace_all` restricted to a literal search and a literal replacement.
    function _replace_all_literal(self, search: string, replacement: string) -> string throws never {
        $rust_function
    }

    // ─── Unicode-aware character-class predicates ─────────────────────────────
    //
    // Every `is_*` predicate below tests that **all** characters in the string
    // satisfy the predicate. The empty string returns `true` (the universal
    // quantifier over an empty set is vacuously true). For single-character
    // strings these behave like character-class checks.

    /// Returns true if every character is numeric per Unicode (general categories
    /// `Nd`, `Nl`, `No` — decimal digits, letter-numbers, and other-numbers).
    /// Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "12345".is_numeric()  // true
    /// "Ⅷ".is_numeric()      // true (Roman numeral, Nl)
    /// "12a".is_numeric()    // false
    /// "".is_numeric()       // true (vacuous)
    /// ```
    function is_numeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is a Unicode letter (general category `L`).
    /// Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "héllo".is_alphabetic()  // true
    /// "漢字".is_alphabetic()    // true
    /// "abc1".is_alphabetic()   // false
    /// ```
    function is_alphabetic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is alphabetic OR numeric per Unicode.
    /// Empty string returns true. Equivalent to checking each char passes
    /// `is_alphabetic` or `is_numeric`.
    ///
    /// # Examples
    /// ```
    /// "abc123".is_alphanumeric()  // true
    /// "héllo7".is_alphanumeric()  // true
    /// "a b".is_alphanumeric()     // false (space is not)
    /// ```
    function is_alphanumeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is uppercase per Unicode (general
    /// category `Lu`, plus other chars with the `Uppercase` property).
    /// Empty string returns true. Note: digits and most symbols are neither
    /// upper- nor lowercase.
    ///
    /// # Examples
    /// ```
    /// "HELLO".is_uppercase()  // true
    /// "Hello".is_uppercase()  // false
    /// "123".is_uppercase()    // false (digits aren't uppercase)
    /// ```
    function is_uppercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is lowercase per Unicode (general
    /// category `Ll`, plus other chars with the `Lowercase` property).
    /// Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "hello".is_lowercase()  // true
    /// "Hello".is_lowercase()  // false
    /// ```
    function is_lowercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is whitespace per Unicode (`White_Space`
    /// property — includes space, tab, newline, NBSP, and other Unicode
    /// whitespace). Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "   ".is_whitespace()      // true
    /// " \t\n".is_whitespace()    // true
    /// "a b".is_whitespace()      // false
    /// ```
    function is_whitespace(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is a control character per Unicode
    /// (general category `Cc`). Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "\n\t".is_control()  // true
    /// "a".is_control()     // false
    /// ```
    function is_control(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is a "graphic" character — that is,
    /// not a control character and not whitespace. Empty string returns true.
    ///
    /// This is a convenience predicate for "visible/printing" characters. It
    /// uses the Unicode definition of control and whitespace.
    ///
    /// # Examples
    /// ```
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

    /// Returns true if every character is ASCII (i.e. has a code point in
    /// `[0x00, 0x7F]`). Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "hello".is_ascii()    // true
    /// "héllo".is_ascii()    // false (`é` is U+00E9)
    /// ```
    function is_ascii(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII decimal digit (`0`..=`9`).
    /// Empty string returns true. Stricter than `is_numeric`, which accepts
    /// non-ASCII numerals.
    ///
    /// # Examples
    /// ```
    /// "12345".is_ascii_numeric()  // true
    /// "Ⅷ".is_ascii_numeric()      // false (Roman numeral)
    /// ```
    function is_ascii_numeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII letter (`A`..=`Z` or
    /// `a`..=`z`). Empty string returns true.
    function is_ascii_alphabetic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII letter or digit. Empty
    /// string returns true.
    function is_ascii_alphanumeric(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII uppercase letter
    /// (`A`..=`Z`). Empty string returns true.
    function is_ascii_uppercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII lowercase letter
    /// (`a`..=`z`). Empty string returns true.
    function is_ascii_lowercase(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is ASCII whitespace: space (`\x20`),
    /// horizontal tab (`\t`), line feed (`\n`), form feed (`\x0C`), or
    /// carriage return (`\r`). Note this is narrower than `is_whitespace`,
    /// which accepts Unicode whitespace like NBSP.
    function is_ascii_whitespace(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII control character — code
    /// points `[0x00, 0x1F]` plus DEL (`0x7F`). Empty string returns true.
    function is_ascii_control(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII "graphic" character —
    /// printable, non-space, non-control: code points `[0x21, 0x7E]`. Note
    /// the ASCII space is *not* graphic. Empty string returns true.
    function is_ascii_graphic(self) -> bool throws never {
        $rust_function
    }

    /// Returns true if every character is an ASCII hexadecimal digit:
    /// `0`..=`9`, `a`..=`f`, or `A`..=`F`. Empty string returns true.
    ///
    /// # Examples
    /// ```
    /// "deadBEEF".is_ascii_hex()  // true
    /// "0x123".is_ascii_hex()     // false (`x` is not a hex digit)
    /// ```
    function is_ascii_hex(self) -> bool throws never {
        $rust_function
    }

    /// Returns the string encoded as a `uint8array` of UTF-8 bytes.
    ///
    /// The resulting byte array has length `self.byte_length()`. Inverse of the
    /// static `string.from_utf8(bytes)`.
    ///
    /// # Examples
    /// ```
    /// "hi".to_utf8()     // [0x68, 0x69]
    /// "é".to_utf8()      // [0xC3, 0xA9]
    /// "".to_utf8()       // []
    /// ```
    function to_utf8(self) -> uint8array throws never {
        $rust_function
    }

    /// Decodes a `uint8array` of UTF-8 bytes into a string.
    ///
    /// Throws `InvalidArgument` if `utf8` is not valid UTF-8. To decode bytes
    /// with replacement characters (lossy), use `uint8array.to_string()`
    /// instead, which substitutes U+FFFD for invalid sequences.
    ///
    /// # Examples
    /// ```
    /// string.from_utf8(b"\x68\x69")          // "hi"
    /// string.from_utf8(b"\xC3\xA9")          // "é"
    /// string.from_utf8(b"\xFF")              // throws — 0xFF is invalid UTF-8
    /// ```
    function from_utf8(utf8: uint8array) -> string throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns the string's Unicode code points as an array of `int`s.
    ///
    /// This is the exact inverse of `string.from_code_points`: for any string
    /// `s`, `string.from_code_points(s.to_code_points())` equals `s`. The result
    /// has one element per character (`self.length()` elements), each in
    /// `[0, 0x10FFFF]`. Never throws.
    ///
    /// Use this for char → integer mappings (checksums, base-N encoding, hashing,
    /// character classification) instead of indexing into a literal alphabet
    /// string.
    ///
    /// # Examples
    /// ```
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
    /// Each value in `unicode` must be in the range `[0, 0x10FFFF]` and must
    /// not be a UTF-16 surrogate (the range `[0xD800, 0xDFFF]`). Throws
    /// `InvalidArgument` on any invalid value, identifying the offending
    /// position in the array.
    ///
    /// # Examples
    /// ```
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

    function add(self, rhs: string) -> string throws never {
        `${self}${rhs}`
    }
}
