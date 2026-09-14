/// YAML text that cannot be read as a single JSON-compatible document.
///
/// `message` is either the parser's own diagnostic for malformed YAML, or the
/// reason a well-formed document falls outside `baml.json.json`.
class ParseError {
    message: string,
}

/// Parses a single YAML document into JSON-compatible BAML values.
///
/// YAML features outside `baml.json.json` are rejected rather than converted
/// lossily. The accepted values are null, bool, int, finite float, string,
/// sequences, and mappings with string keys. Where `baml.json.parse` degrades
/// an integer outside the BAML `int` range to `float`, an out-of-range YAML
/// integer is rejected.
///
/// The input is a YAML *stream*, and this reads exactly one document from it.
/// An empty stream is not malformed and parses to `null`. A second document —
/// a `---` separator with content after it — is an error rather than a
/// silently discarded remainder.
///
/// # Throws
///
/// - `baml.yaml.ParseError` if `s` is not well-formed YAML, if the stream
///   holds more than one document, if a mapping key is not a string, if a node
///   carries an explicit tag, if an integer falls outside the BAML `int`
///   range, or if a float is not finite.
//baml:mut_vm
function parse(s: string) -> baml.json.json throws ParseError {
    $rust_function
}

/// Parses YAML text and decodes it into `T`, honoring `baml.FromJson`
/// overrides. The YAML counterpart of `baml.json.deserialize<T>`.
///
/// `parse` lands in `baml.json.json` first, so the decode rules are exactly
/// `baml.json`'s — see `baml.json.to<T>`.
///
/// # Throws
///
/// - `baml.yaml.ParseError` for anything `parse` rejects.
/// - `baml.json.DecodeError` if the parsed document does not fit `T`. Decoding
///   an already-parsed `json` never re-parses, so no `baml.json.ParseError`
///   arises here.
function deserialize<T>(s: string) -> T throws ParseError | baml.json.DecodeError {
    baml.json.from_json<T>(parse(s))
}
