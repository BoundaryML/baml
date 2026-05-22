/// The null type, representing the absence of a value.
class Null {
  /// Serializes null to JSON null.
  function to_json(self) -> baml.json.json {
    $rust_function
  }
}
