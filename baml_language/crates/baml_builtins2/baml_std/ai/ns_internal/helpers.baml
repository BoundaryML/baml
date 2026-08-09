// Internal helpers. Nothing here is part of the public ai surface.

// One JSON Schema object for a function's parameters, derived from the
// signature: every parameter typed via baml.schema.json_schema.
function _schema_for_signature(handler: baml.AnyFunction) -> map<string, unknown> {
    let sig = reflect.signature(handler);
    let props: map<string, unknown> = {};
    let required: string[] = [];
    for (let a in sig.args) {
        let _ = props.set(a.name, baml.schema.json_schema(a.type));
        required.push(a.name);
    }
    let schema: map<string, unknown> = {};
    let _ = schema.set("type", "object");
    let _ = schema.set("properties", props);
    let _ = schema.set("required", required);
    let _ = schema.set("additionalProperties", false);
    schema
}

// Bound as `ctx` inside a compiler-desugared `@spec` prompt template.
// `output_format` is the one placeholder the BEP allows.
class SpecCtx {
    output_format: string,
}
