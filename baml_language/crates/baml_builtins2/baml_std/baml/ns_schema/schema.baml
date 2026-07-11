// Provider-neutral JSON Schema lowering for BAML `type` values. Provider
// implementations can inspect and transform the returned JSON tree for their
// own schema dialect instead of parsing a serialized schema string.

/// Lower a BAML `type` to ordinary JSON Schema. Optional BAML fields are omitted
/// from `required`; maps use schema-valued `additionalProperties`; unions use
/// `anyOf`; recursive class graphs use `$defs` and `$ref`. Providers with a
/// restricted dialect should transform this value at their wire boundary and
/// reject shapes that their API cannot represent.
///
/// Throws `root.errors.Unsupported` when the type contains a construct with no JSON
/// Schema representation (functions, opaque rust types, unresolved generic params).
function json_schema(t: type) -> root.json.json throws root.errors.Unsupported {
    $rust_io_function
}
