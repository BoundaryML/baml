// Provider-neutral JSON Schema lowering for BAML runtime types.

/// Lower a BAML `type` to ordinary JSON Schema. Optional fields are omitted
/// from `required`, maps use schema-valued `additionalProperties`, unions use
/// `anyOf`, and recursive class graphs use `$defs` and `$ref`.
///
/// Throws `root.errors.Unsupported` for constructs without a JSON Schema form.
function json_schema(t: type) -> root.json.json throws root.errors.Unsupported {
    $rust_io_function
}
