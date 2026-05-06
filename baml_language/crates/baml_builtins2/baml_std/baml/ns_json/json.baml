type json = null | bool | int | float | string | json[] | map<string, json>

class JsonParseError {
  message string
}
class JsonDecodeError {
  message string
  path string
}
class JsonSerializationError {
  message string
  path string
  reason string
}

//baml:mut_vm
function parse(s string) -> json throws JsonParseError {
  $rust_function
}

//baml:mut_vm
function stringify(j json) -> string {
  $rust_function
}

//baml:mut_vm
function stringify_pretty(j json) -> string {
  $rust_function
}

/// Serialize any BAML value `v` of type `T` to its canonical JSON string.
///
/// The type argument `T` is delivered to the native handler at runtime via
/// the BEP-039 type-arg channel, so the walker can produce a tagged JSON
/// document for media values, the variant name for enums, and the natural
/// JSON shape for primitives / lists / maps / classes.
///
/// Throws `JsonSerializationError` for non-representable types
/// (`uint8array` without explicit encoding, function values, etc.).
//baml:mut_vm
function to_string<T>(v: T) -> string throws JsonSerializationError {
  $rust_function
}

/// Serialize any BAML value to its `json` representation by dispatching on
/// the value's runtime type.
///
/// Equivalent to `v.to_json()` when `T` is concrete, but works through generic
/// boundaries: a synthesized `to_json` body on `class Box<T> { value T }` can
/// call `baml.json.to_json(self.value)` to get override-honoring dispatch
/// without knowing `T` at synthesis time. The auto-derive synthesizer
/// (`baml_compiler2_ast::auto_derive_json`) uses this for `TypeVar`-typed
/// fields where direct method dispatch would fail at runtime (the receiver's
/// concrete `to_json` only exists on the runtime value, not the static type).
///
/// Behaviour matches the per-type `to_json`: dispatches to `Int.to_json`,
/// `String.to_json`, user-class `to_json` (including overrides),
/// `Array<T>.to_json`, etc.
//baml:mut_vm
//baml:may_yield
function to_json<T>(v: T) -> json throws JsonSerializationError | JsonParseError {
  $rust_function
}

/// Parse a JSON string and coerce it to a BAML value of type `T`.
///
/// The type argument `T` is delivered to the native handler at runtime via
/// the BEP-039 type-arg channel.  The handler parses the string with
/// `serde_json`, then runs SAP's coercer against the runtime `Ty` to produce
/// a typed value.
///
/// Throws `JsonParseError` if the string is not valid JSON, or
/// `JsonDecodeError` if the parsed JSON does not match `T`.
//baml:mut_vm
function from_string<T>(s: string) -> T throws JsonParseError | JsonDecodeError {
  $rust_function
}
