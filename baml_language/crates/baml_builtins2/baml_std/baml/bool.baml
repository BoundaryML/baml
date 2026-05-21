/// A boolean value — either `true` or `false`.
class Bool {
  /// Serializes this boolean to a JSON boolean.
  function to_json(self) -> baml.json.json {
    $rust_function
  }
}
