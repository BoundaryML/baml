// JSON Schema lowering for BAML `type` values. The one leaf host function here
// (`json_schema`) walks a runtime type and emits a JSON Schema string. With
// `strict: true` the schema follows OpenAI structured-output ("strict mode")
// rules — this is what `baml.ai.OpenAiStrict` sends as `response_format`.

/// Lower a BAML `type` to a JSON Schema string. With `strict: true` the schema follows
/// OpenAI strict-mode rules: every object gets `"additionalProperties": false` and ALL
/// fields listed in `required` (optional BAML fields become `{"type": [..., "null"]}`
/// unions instead of being omitted from required).
///
/// Throws `root.errors.Unsupported` when the type contains a construct with no JSON
/// Schema representation (functions, opaque rust types, unresolved generic params).
function json_schema(t: type, strict: bool) -> string throws root.errors.Unsupported {
    $rust_io_function
}
