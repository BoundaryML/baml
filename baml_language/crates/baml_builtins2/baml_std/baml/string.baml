/// A UTF-8 encoded string.
///
/// String literals use double quotes: `"hello"`. Multiline strings use the `#"..."#` syntax.
/// All methods returning a new string do not mutate `self` unless noted otherwise.
class String {
  /// Serializes this string to a JSON value.
  //baml:mut_vm
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns the length of the string in **UTF-8 bytes**.
  ///
  /// This is `O(1)`. For ASCII text the byte length equals the character
  /// count, but for any string containing multi-byte characters the two
  /// differ — e.g. `"héllo".length() == 6` because `é` is two bytes.
  ///
  /// To count Unicode code points (`O(n)`), use `char_count()`.
  ///
  /// # Examples
  /// ```
  /// "hello".length()  // 5
  /// "héllo".length()  // 6 (é is 2 bytes in UTF-8)
  /// "".length()       // 0
  /// ```
  function length(self) -> int {
    $rust_function
  }

  /// Returns the number of **Unicode code points** (characters) in the string.
  ///
  /// This is `O(n)` because it iterates the string. For ASCII text it equals
  /// `length()`, but for strings with multi-byte UTF-8 characters they differ.
  ///
  /// Note: a code point is not always one user-perceived character — e.g. a
  /// flag emoji like `🇺🇸` is composed of two code points, and combining marks
  /// (like `é` written as `e` + U+0301) count as multiple. For most practical
  /// purposes (counting characters in Latin/CJK text) this returns what you
  /// expect.
  ///
  /// # Examples
  /// ```
  /// "hello".char_count()  // 5
  /// "héllo".char_count()  // 5  (`length()` is 6)
  /// "🐑".char_count()  // 1  (`length()` is 4)
  /// "".char_count()       // 0
  /// ```
  function char_count(self) -> int {
    $rust_function
  }

  /// Returns the string with all Unicode characters converted to lowercase.
  function to_lower_case(self) -> string {
    $rust_function
  }

  /// Returns the string with all Unicode characters converted to uppercase.
  function to_upper_case(self) -> string {
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
  function trim(self) -> string {
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
  function trim_start(self) -> string {
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
  function trim_end(self) -> string {
    $rust_function
  }

  /// Returns true if `search` appears anywhere in the string.
  function includes(self, search: string) -> bool {
    $rust_function
  }

  /// Returns true if the string starts with `prefix`.
  function starts_with(self, prefix: string) -> bool {
    $rust_function
  }

  /// Returns true if the string ends with `suffix`.
  function ends_with(self, suffix: string) -> bool {
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
  function split(self, delimiter: string) -> string[] {
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
  function lines(self) -> string[] {
    $rust_function
  }

  /// Returns the substring between two **byte** offsets `[start, end)`.
  ///
  /// Both `start` and `end` are byte indices, matching `length()`. They are
  /// clamped to the range `[0, length()]`, and `end` is further clamped so
  /// that `end >= start` (an empty string is returned if `end < start` after
  /// clamping).
  ///
  /// Throws `InvalidArgument` if either clamped offset falls in the middle
  /// of a multi-byte UTF-8 character, since the result would not be a valid
  /// string.
  ///
  /// # Examples
  /// ```
  /// "hello world".substring(0, 5)   // "hello"
  /// "hello".substring(2, 100)       // "llo" (end clamped)
  /// "héllo".substring(0, 3)         // "hé"
  /// "héllo".substring(0, 2)         // throws — index 2 is mid-`é`
  /// ```
  function substring(self, start: int, end: int) -> string throws root.errors.InvalidArgument {
    $rust_function
  }

  /// Replaces the first occurrence of `search` with `replacement`.
  function replace(self, search: string, replacement: string) -> string {
    $rust_function
  }

  /// Returns the **byte offset** of the first occurrence of `search`, or `-1`
  /// if not found. The returned offset is suitable for use with `substring`,
  /// `char_at`, and `code_point_at`.
  ///
  /// # Examples
  /// ```
  /// "hello world".index_of("world")  // 6
  /// "héllo".index_of("l")            // 3 (after the 2-byte `é`)
  /// "abc".index_of("z")              // -1
  /// ```
  function index_of(self, search: string) -> int {
    $rust_function
  }

  /// Returns the single character that starts at the given **byte offset**,
  /// as a one-character string.
  ///
  /// `index` is a byte index (matching `length()`). If `index == length()`
  /// the result is an empty string. Throws `InvalidArgument` if `index` is
  /// negative, beyond `length()`, or falls in the middle of a multi-byte
  /// UTF-8 character.
  ///
  /// # Examples
  /// ```
  /// "hello".char_at(0)   // "h"
  /// "héllo".char_at(1)   // "é"
  /// "héllo".char_at(3)   // "l" (the `é` occupies bytes 1..3)
  /// "héllo".char_at(2)   // throws — index 2 is mid-`é`
  /// "hi".char_at(2)      // ""  (one past the end)
  /// ```
  function char_at(self, index: int) -> string throws root.errors.InvalidArgument {
    $rust_function
  }

  /// Returns the Unicode code point starting at the given **byte offset** as
  /// an integer in the range `[0, 0x10FFFF]`.
  ///
  /// `index` is a byte index (matching `length()`). Throws `InvalidArgument`
  /// if `index` is negative, beyond `length() - 1`, or falls in the middle of
  /// a multi-byte UTF-8 character. Note that, unlike `char_at`, requesting the
  /// position one past the end (`index == length()`) is also an error since
  /// there is no code point there.
  ///
  /// # Examples
  /// ```
  /// "hi".code_point_at(0)     // 104  (ASCII 'h')
  /// "héllo".code_point_at(1)  // 233  (U+00E9 'é')
  /// "🐑".code_point_at(0)     // 128017 (U+1F411)
  /// "héllo".code_point_at(2)  // throws — index 2 is mid-`é`
  /// ```
  function code_point_at(self, index: int) -> int throws root.errors.InvalidArgument {
    $rust_function
  }

  /// Returns true if the string contains the given `substring`.
  function matches(self, substring: string) -> bool {
    $rust_function
  }

  /// Replaces all occurrences of `search` with `replacement`.
  function replace_all(self, search: string, replacement: string) -> string {
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
  function is_numeric(self) -> bool {
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
  function is_alphabetic(self) -> bool {
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
  function is_alphanumeric(self) -> bool {
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
  function is_uppercase(self) -> bool {
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
  function is_lowercase(self) -> bool {
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
  function is_whitespace(self) -> bool {
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
  function is_control(self) -> bool {
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
  function is_graphic(self) -> bool {
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
  function is_ascii(self) -> bool {
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
  function is_ascii_numeric(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII letter (`A`..=`Z` or
  /// `a`..=`z`). Empty string returns true.
  function is_ascii_alphabetic(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII letter or digit. Empty
  /// string returns true.
  function is_ascii_alphanumeric(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII uppercase letter
  /// (`A`..=`Z`). Empty string returns true.
  function is_ascii_uppercase(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII lowercase letter
  /// (`a`..=`z`). Empty string returns true.
  function is_ascii_lowercase(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is ASCII whitespace: space (`\x20`),
  /// horizontal tab (`\t`), line feed (`\n`), form feed (`\x0C`), or
  /// carriage return (`\r`). Note this is narrower than `is_whitespace`,
  /// which accepts Unicode whitespace like NBSP.
  function is_ascii_whitespace(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII control character — code
  /// points `[0x00, 0x1F]` plus DEL (`0x7F`). Empty string returns true.
  function is_ascii_control(self) -> bool {
    $rust_function
  }

  /// Returns true if every character is an ASCII "graphic" character —
  /// printable, non-space, non-control: code points `[0x21, 0x7E]`. Note
  /// the ASCII space is *not* graphic. Empty string returns true.
  function is_ascii_graphic(self) -> bool {
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
  function is_ascii_hex(self) -> bool {
    $rust_function
  }

  /// Returns the string encoded as a `uint8array` of UTF-8 bytes.
  ///
  /// The resulting byte array has length `self.length()`. Inverse of the
  /// static `string.from_utf8(bytes)`.
  ///
  /// # Examples
  /// ```
  /// "hi".to_utf8()     // [0x68, 0x69]
  /// "é".to_utf8()      // [0xC3, 0xA9]
  /// "".to_utf8()       // []
  /// ```
  function to_utf8(self) -> uint8array {
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
