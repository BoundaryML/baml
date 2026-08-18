// Internal helpers. Nothing here is part of the public ai surface.

// One JSON Schema object for a function's parameters, derived from the
// signature: every parameter typed via baml.json.schema.
function _schema_for_signature(handler: baml.AnyFunction) -> map<string, unknown> {
    let sig = reflect.signature(handler);
    let props: map<string, unknown> = {};
    let required: string[] = [];
    for (let a in sig.args) {
        let _ = props.set(a.name, baml.json.schema(a.type));
        required.push(a.name);
    }
    { "type": "object", "properties": props, "required": required, "additionalProperties": false }
}

// Bound as `ctx` inside a compiler-desugared `@spec` prompt template.
// `output_format` is the one placeholder the BEP allows.
class SpecCtx {
    output_format: string,
}

/// Assemble a structural `ai.Prompt` from a tagged template's flattened
/// `parts` and `values`. `root.Role` values start new messages; media
/// remains structural until a provider lowers the prompt.
//baml:mut_vm
//baml:may_yield
function assemble_prompt(parts: string[], values: unknown[]) -> root.Prompt throws never {
    $rust_function
}
