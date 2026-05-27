/// A boolean value — either `true` or `false`.
class Bool {
  /// Serializes this boolean to a JSON boolean.
  function to_json(self) -> baml.json.json {
    $rust_function
  }

  /// Returns `"true"` or `"false"`. Used by BEP-049 backtick-string
  /// interpolation (§11) to implicitly convert boolean values appearing
  /// in `${...}`.
  function to_string(self) -> string {
    $rust_function
  }
}
