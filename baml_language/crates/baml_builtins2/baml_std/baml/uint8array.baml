class Uint8Array {
  function length(self) -> int {
    $rust_function
  }

  function at(self, index: int) -> int? {
    $rust_function
  }

  //baml:mut_self
  function push(self, item: int) -> int {
    $rust_function
  }

  //baml:mut_self
  function pop(self) -> int? {
    $rust_function
  }

  /// Returns a new `uint8array` containing the concatenation of the two arrays.
  function concat(self, other: uint8array) -> uint8array {
    $rust_function
  }

  /// Returns `true` if the array contains the given number.
  /// If the item is not a `u8`, this will always return `false`.
  function includes(self, item: int) -> bool {
    $rust_function
  }

  /// Returns a new `uint8array` with the bytes in reverse order.
  function reverse(self) -> uint8array {
    $rust_function
  }

  /// Returns a new `uint8array` containing the bytes in the given range.
  ///
  /// - `start` is clamped to the range `[0, length]`.
  /// - `end` is clamped to the range `[start, length]`.
  function slice(self, start: int, end: int) -> uint8array {
    $rust_function
  }

  /// Creates a new `uint8array` of the given size, filled with zeros.
  ///
  /// Throws an error if the `size` is out of range (e.g. negative).
  /// Panics if the allocation would cause an OOM.
  function zeroes(size: int) -> uint8array throws root.errors.InvalidArgument {
    $rust_function
  }

  function from_array(array: int[]) -> uint8array throws root.errors.InvalidArgument {
    $rust_function
  }

  function to_array(self) -> int[] {
    $rust_function
  }

  function from_hex(hex: string) -> uint8array throws root.errors.InvalidArgument {
    $rust_function
  }

  function to_hex(self) -> string {
    $rust_function
  }

  function from_base64(base64: string) -> uint8array throws root.errors.InvalidArgument {
    $rust_function
  }

  function to_base64(self) -> string {
    $rust_function
  }

  // function map<E>(self, f: (int) -> int throws E) -> uint8array throws E {
  //   $rust_function
  // }

  /// Sorts the array in place.
  //baml:mut_self
  function sort(self) -> null {
    $rust_function
  }
}
