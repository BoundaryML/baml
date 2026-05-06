class String {
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

  function to_lower_case(self) -> string {
    $rust_function
  }

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

  function includes(self, search: string) -> bool {
    $rust_function
  }

  function starts_with(self, prefix: string) -> bool {
    $rust_function
  }

  function ends_with(self, suffix: string) -> bool {
    $rust_function
  }

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

  function matches(self, pattern: string) -> bool {
    $rust_function
  }

  function replace_all(self, search: string, replacement: string) -> string {
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
  /// string.from_code_points([0xD800])           // throws — surrogate
  /// ```
  function from_code_points(unicode: int[]) -> string throws root.errors.InvalidArgument {
    $rust_function
  }
}
