/// Thrown when YAML text cannot be parsed as a single JSON-compatible YAML
/// document.
class YamlParseError {
    message: string,
}

/// Parses a single YAML document into JSON-compatible BAML values.
///
/// YAML features outside `baml.json.json` are rejected instead of converted
/// lossily. Accepted values are null, bool, int, finite float, string,
/// sequences, and mappings with string keys. Unlike `baml.json.parse`, which
/// degrades integers outside the BAML `int` range to `float`, out-of-range
/// YAML integers are rejected.
//baml:mut_vm
function parse(s: string) -> baml.json.json throws YamlParseError {
    $rust_function
}

/// Parses YAML text and decodes it to `T`, honoring user-defined `baml.FromJson`
/// overrides (via `baml.json.from_json<T>`). The YAML counterpart of
/// `baml.json.deserialize<T>`. Parsing throws `YamlParseError`; decoding the
/// resulting `json` into `T` throws `JsonDecodeError` (decoding never re-parses,
/// so no `JsonParseError` arises here).
function deserialize<T>(s: string) -> T throws YamlParseError | baml.json.JsonDecodeError {
    baml.json.from_json<T>(parse(s))
}
