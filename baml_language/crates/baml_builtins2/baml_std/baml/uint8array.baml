/// A mutable, growable array of bytes (u8 values in the range 0–255).
///
/// Used for binary data such as file contents, network payloads, and encoded strings.
/// `push` silently masks values to u8; `from_array` throws `InvalidArgument` for out-of-range values.
class Uint8Array {
    /// Returns the number of bytes in the array.
    function length(self) -> int throws never {
        $rust_function
    }

    /// Returns the byte at `index`, or `null` if out of bounds.
    ///
    /// Negative indices count from the end: `-1` is the last byte.
    function at(self, index: int) -> int? throws never {
        $rust_function
    }

    /// Appends `item` to the end of the array. Values outside 0–255 are silently masked to u8. Returns the new length. Mutates `self`.
    //baml:mut_self
    function push(self, item: int) -> int throws never {
        $rust_function
    }

    /// Removes and returns the last byte, or `null` if the array is empty. Mutates `self`.
    //baml:mut_self
    function pop(self) -> int? throws never {
        $rust_function
    }

    /// Returns a new `uint8array` containing the concatenation of the two arrays.
    function concat(self, other: uint8array) -> uint8array throws never {
        $rust_function
    }

    /// Returns `true` if the array contains the given number.
    /// If the item is not a `u8`, this will always return `false`.
    function includes(self, item: int) -> bool throws never {
        $rust_function
    }

    /// Returns a new `uint8array` with the bytes in reverse order.
    function reverse(self) -> uint8array throws never {
        $rust_function
    }

    /// Returns a new `uint8array` with the bytes from `start` (inclusive) to
    /// `end` (exclusive).
    ///
    /// Negative indices count from the end. Out-of-range indices are clamped, and
    /// an `end` that resolves at or before `start` yields an empty array.
    function slice(self, start: int, end: int) -> uint8array throws never {
        $rust_function
    }

    /// Creates a new `uint8array` of the given size, filled with zeros.
    ///
    /// Throws an error if the `size` is out of range (e.g. negative).
    /// Panics if the allocation would cause an OOM.
    function zeroes(size: int) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Creates a `uint8array` from an array of integers.
    ///
    /// Throws `InvalidArgument` if any value is outside the range 0–255.
    function from_array(array: int[]) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Returns the bytes as an array of integers, each in the range 0–255.
    function to_array(self) -> int[] throws never {
        $rust_function
    }

    /// Decodes a hexadecimal string (e.g. `"deadbeef"`) into bytes.
    ///
    /// Throws `InvalidArgument` if the input contains non-hex characters or has an odd length.
    function from_hex(hex: string) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Encodes the bytes as a lowercase hexadecimal string (e.g. `"deadbeef"`).
    function to_hex(self) -> string throws never {
        $rust_function
    }

    /// Decodes a standard Base64-encoded string into bytes.
    ///
    /// Accepts both standard (`+/`) and URL-safe (`-_`) alphabets, with or without padding.
    ///
    /// # Examples
    /// ```
    /// uint8array.from_base64("aGVsbG8=")   // [104, 101, 108, 108, 111]  ("hello")
    /// ```
    ///
    /// Throws `InvalidArgument` if the input is not valid Base64.
    function from_base64(base64: string) -> uint8array throws root.errors.InvalidArgument {
        $rust_function
    }

    /// Encodes the bytes as a standard Base64 string (with `=` padding).
    ///
    /// # Examples
    /// ```
    /// "hello".to_utf8().to_base64()   // "aGVsbG8="
    /// ```
    function to_base64(self) -> string throws never {
        $rust_function
    }

    /// Decodes the bytes as a UTF-8 string.
    /// Invalid UTF-8 sequences are replaced with U+FFFD (replacement character).
    implements baml.ToString {
        function to_string(self) -> string throws never {
            self._to_string_impl()
        }
    }

    function _to_string_impl(self) -> string throws never {
        $rust_function
    }

    // function map<E>(self, f: (int) -> int throws E) -> uint8array throws E {
    //   $rust_function
    // }

    /// Sorts the array in place.
    //baml:mut_self
    function sort(self) -> null throws never {
        $rust_function
    }
}
