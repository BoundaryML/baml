/// The BAML JSON value type — a union of all JSON-representable values.
type json = null | bool | int | float | string | json[] | map<string, json>

/// Thrown when a JSON string cannot be parsed (syntax error).
class JsonParseError {
  message string
}
/// Thrown when a parsed JSON value does not match the expected BAML type.
class JsonDecodeError {
  message string
  path string
}
/// Thrown when a BAML value cannot be serialized to JSON.
class JsonSerializationError {
  message string
  path string
  reason string
}

//baml:mut_vm
/// Parses a JSON string `s` and returns a `json` value. Throws `JsonParseError` on invalid JSON.
function parse(s string) -> json throws JsonParseError {
  $rust_function
}

//baml:mut_vm
/// Serializes a `json` value `j` to a compact JSON string.
function stringify(j json) -> string {
  $rust_function
}

//baml:mut_vm
/// Serializes a `json` value `j` to a pretty-printed (indented) JSON string.
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

/// Decode a `json` value to a BAML value of type `T`, honoring user
/// `from_json` overrides on classes (including through arrays, maps, and
/// optionals).
///
/// Equivalent to `T.from_json(j)` when `T` is a concrete class, but works
/// through generic boundaries: a synthesized `from_json` body on
/// `class Box<T> { value T }` calls `baml.json.from_json<T>(...)` to
/// dispatch on the runtime substitution of `T` without knowing it at
/// synthesis time. The auto-derive synthesizer
/// (`baml_compiler2_ast::auto_derive_json`) routes every field through
/// this function so override dispatch is centralized.
///
/// Behaviour by type-arg:
/// - User class with `from_json` method: dispatch to that method.
/// - List / Map / Optional of class: walk structurally and dispatch the
///   inner class's `from_json` per element / value.
/// - Primitives, enums, media, type-aliases: structural decode equivalent
///   to `from_string<T>(stringify(j))`.
///
/// Throws `JsonParseError` / `JsonDecodeError` on shape mismatch.
//baml:mut_vm
//baml:may_yield
function from_json<T>(j: json) -> T throws JsonParseError | JsonDecodeError {
  $rust_function
}

/// Look up `key` in a `json` value when it is a `map<string, json>`; returns
/// `null` if `j` is not a map or the key is absent.
///
/// Used by the auto-derive `from_json` synthesizer to extract field values
/// from a `json` object without requiring the caller to match-narrow the
/// `json` union to its map arm.
//baml:mut_vm
function field(j: json, key: string) -> json {
  $rust_function
}

/// Serialize `v` to JSON text, honoring user-defined `to_json` overrides on
/// classes (via [`to_json<T>`]). Equivalent to `stringify(to_json<T>(v))`.
///
/// Use this from a host that wants to print a target's return value as JSON
/// while still respecting class-level `to_json` overrides. The structural
/// path is [`to_string<T>`], which walks fields directly and bypasses
/// overrides — appropriate when override behavior would be a problem.
///
/// HACK: this is a thin shim that only exists so `baml_exec::dispatch`
/// has a stable named entry point to call from Rust (via
/// `engine.call_function("baml.json.serialize", ...)`). The composition
/// `stringify(to_json<T>(v))` is what callers would write inline anyway;
/// owning a wrapper function gives us one engine-side symbol to look up
/// instead of two, and a place to evolve the override semantics if they
/// drift. Remove this if the host gains a direct way to invoke generic
/// compositions without a named landing pad.
function serialize<T>(v: T) -> string throws JsonSerializationError | JsonParseError {
  stringify(to_json<T>(v))
}

/// Parse JSON text into a value of type `T`, honoring user-defined
/// `from_json` overrides on classes (via [`from_json<T>`]). Equivalent to
/// `from_json<T>(parse(s))`.
///
/// Use this from a host that wants to coerce a JSON-shaped input (such as
/// `--json-args`) into a typed BAML value while respecting class-level
/// `from_json` overrides. The structural path is [`from_string<T>`], which
/// decodes structurally and bypasses overrides.
///
/// HACK: same story as [`serialize<T>`] — this wrapper exists so
/// `baml_exec::dispatch::build_args_from_signature` can resolve a single
/// stable name (`baml.json.deserialize`) to coerce `--json-args` payloads
/// through `from_json<T>(parse(s))`. Inlining the composition into the
/// Rust caller would mean threading two generic engine calls + their
/// throws-types instead of one, so the wrapper pays for itself.
function deserialize<T>(s: string) -> T throws JsonParseError | JsonDecodeError {
  from_json<T>(parse(s))
}
