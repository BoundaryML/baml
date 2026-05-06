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
  /// # Examples
  /// ```
  /// "hello".length()  // 5
  /// "héllo".length()  // 6 (é is 2 bytes in UTF-8)
  /// "".length()       // 0
  /// ```
  function length(self) -> int {
    $rust_function
  }

  function to_lower_case(self) -> string {
    $rust_function
  }

  function to_upper_case(self) -> string {
    $rust_function
  }

  function trim(self) -> string {
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

  function matches(self, pattern: string) -> bool {
    $rust_function
  }

  function replace_all(self, search: string, replacement: string) -> string {
    $rust_function
  }

  /// Encodes the string as UTF-8 bytes.
  function to_bytes(self) -> uint8array {
    $rust_function
  }
}
