/// A mutable, growable array of bytes, each in the range 0–255.
///
/// The type for binary data: file contents, network payloads, encoded strings.
/// `push`, `pop`, and `sort` modify the array in place; every other method here
/// returns a new one.
///
/// The two ways of turning an `int` into a byte disagree deliberately. `push`
/// silently masks its argument to the low 8 bits, as assigning into a JavaScript
/// typed array does, while `from_array` throws `baml.errors.InvalidArgument` on
/// anything outside 0–255.
///
/// JavaScript's [`Uint8Array`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array),
/// Python's [`bytearray`](https://docs.python.org/3/library/stdtypes.html#bytearray).
class Uint8Array {
    /// Returns the number of bytes.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns the byte at `index`, or `null` if `index` is out of range.
    ///
    /// A negative `index` counts from the end, so `-1` is the last byte. This is
    /// JavaScript's [`at`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/TypedArray/at)
    /// with `null` in place of `undefined`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"abc".at(0)   // 97
    /// b"abc".at(-1)  // 99
    /// b"abc".at(99)  // null
    /// ```
    function at(self, index: int) -> int? throws never {
        $rust_function
    }

    /// Appends `item` to the end of the array and returns the new length.
    ///
    /// `item` is masked to its low 8 bits rather than rejected: `256` appends `0`,
    /// and `-1` appends `255`. Use `from_array` when out-of-range input should be
    /// an error instead.
    //baml:mut_self
    function push(self, item: int) -> int throws never {
        $rust_function
    }

    /// Removes and returns the last byte, or `null` if the array is empty.
    //baml:mut_self
    function pop(self) -> int? throws never {
        $rust_function
    }

    /// Returns a new array holding `self` followed by `other`.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"abc".concat(b"def")  // [97, 98, 99, 100, 101, 102]
    /// ```
    function concat(self, other: uint8array) -> uint8array throws never {
        $rust_function
    }

    /// Returns true if the array contains `item`.
    ///
    /// An `item` outside 0–255 is never present.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"abc".includes(98)  // true
    /// ```
    function includes(self, item: int) -> bool throws never {
        $rust_function
    }

    /// Returns the index of the first occurrence of `item`, or `null` if it is
    /// absent. An `item` outside 0–255 is never present.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"abc".index_of(98)   // 1
    /// b"abc".index_of(122)  // null
    /// ```
    function index_of(self, item: int) -> int? throws never {
        $rust_function
    }

    /// Returns a new array with the bytes in reverse order.
    ///
    /// JavaScript's [`reverse`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/TypedArray/reverse)
    /// reverses in place; this leaves `self` alone.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"abc".reverse()  // b"cba"
    /// ```
    function reverse(self) -> uint8array throws never {
        $rust_function
    }

    /// Returns the bytes in `[start, end)`.
    ///
    /// Either bound may be negative, counting from the end. Out-of-range bounds
    /// are clamped, and an `end` at or before `start` yields an empty array.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"hello".slice(0, 3)    // b"hel"
    /// b"hello".slice(-3, -1)  // b"ll"
    /// ```
    function slice(self, start: int, end: int) -> uint8array throws never {
        $rust_function
    }

    /// Creates an array of `size` zero bytes.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `size` is negative.
    ///
    /// # Panics
    ///
    /// - `baml.panics.AllocFailure` if `size` bytes cannot be allocated.
    ///
    /// # Examples
    ///
    /// ```baml
    /// uint8array.zeroes(3)  // [0, 0, 0]
    /// uint8array.zeroes(0)  // []
    /// ```
    function zeroes(size: int) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Creates a `uint8array` from an array of integers, one byte per element.
    ///
    /// Unlike `push`, this validates rather than masks.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if any value is outside the range 0–255.
    ///
    /// # Examples
    ///
    /// ```baml
    /// uint8array.from_array([104, 101, 108, 108, 111])  // b"hello"
    /// uint8array.from_array([256])                      // throws — out of range
    /// ```
    function from_array(array: int[]) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns the bytes as `int`s, each in the range 0–255.
    ///
    /// # Examples
    ///
    /// ```baml
    /// b"hi".to_array()  // [104, 105]
    /// ```
    function to_array(self) -> int[] throws never {
        $rust_function
    }

    /// Decodes a hexadecimal string such as `"deadbeef"` into bytes.
    ///
    /// Two characters per byte, in either case. Python's
    /// [`bytes.fromhex`](https://docs.python.org/3/library/stdtypes.html#bytes.fromhex)
    /// additionally skips embedded whitespace; this does not.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `hex` has an odd length or contains a
    ///   non-hex character.
    ///
    /// # Examples
    ///
    /// ```baml
    /// uint8array.from_hex("deadbeef")  // [222, 173, 190, 239]
    /// ```
    function from_hex(hex: string) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Encodes the bytes as a lowercase hexadecimal string, two characters per
    /// byte. Python's [`bytes.hex`](https://docs.python.org/3/library/stdtypes.html#bytes.hex).
    ///
    /// # Examples
    ///
    /// ```baml
    /// uint8array.from_hex("deadbeef").to_hex()  // "deadbeef"
    /// ```
    function to_hex(self) -> string throws never {
        $rust_function
    }

    /// Decodes a Base64-encoded string into bytes.
    ///
    /// Accepts both the standard (`+/`) and URL-safe (`-_`) alphabets, with or
    /// without padding.
    ///
    /// # Throws
    ///
    /// - `baml.errors.InvalidArgument` if `base64` is not valid Base64.
    ///
    /// # Examples
    ///
    /// ```baml
    /// uint8array.from_base64("aGVsbG8=")   // [104, 101, 108, 108, 111]  ("hello")
    /// ```
    function from_base64(base64: string) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Encodes the bytes as a standard Base64 string, with `=` padding. Python's
    /// [`base64.b64encode`](https://docs.python.org/3/library/base64.html#base64.b64encode).
    ///
    /// # Examples
    ///
    /// ```baml
    /// "hello".to_utf8().to_base64()   // "aGVsbG8="
    /// ```
    function to_base64(self) -> string throws never {
        $rust_function
    }

    implements baml.ToString {
        /// Lossily decodes the bytes as UTF-8.
        ///
        /// Each invalid sequence becomes U+FFFD, the replacement character. Use
        /// `string.from_utf8` to reject malformed input instead.
        ///
        /// # Examples
        ///
        /// ```baml
        /// b"hello".to_string()  // "hello"
        /// b"".to_string()  // ""
        /// b"\x80".to_string()  // "�"
        /// ```
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    /// (internal) Native backing for the `to_string` implementation above.
    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    // function map<E>(self, f: (int) -> int throws E) -> uint8array throws E {
    //   $rust_function
    // }

    /// Sorts the bytes in place, ascending.
    //baml:mut_self
    function sort(self) -> null throws never {
        $rust_function
    }
}
