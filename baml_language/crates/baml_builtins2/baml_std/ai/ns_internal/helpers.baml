// Internal helpers. Nothing here is part of the public ai surface.

// One JSON Schema object for a function's parameters, derived from the
// signature: every parameter typed via baml.json.schema.
function _schema_for_signature(handler: reflect.AnyFunction) -> map<string, unknown> {
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

/// Assemble a compiler-generated LLM prompt while preserving the legacy
/// default for wholly role-less prompts and implicit leading content before a
/// later explicit role. Standalone `ai.prompt` remains role-less when its
/// author does not opt into roles; this compatibility rule applies only to
/// declarative `prompt:` bodies.
function assemble_llm_prompt(parts: string[], values: unknown[]) -> root.Prompt throws never {
    // The generated tagged-template representation alternates `parts` and
    // `values`. A role marker at the very beginning may be preceded by the
    // indentation/newline used to format a prompt; that whitespace is not a
    // substantive prelude. Any earlier interpolation is content, even when
    // its eventual string representation is not known here.
    let first_role = -1;
    let i = 0;
    while (i < values.length()) {
        if (first_role < 0 && values[i] is root.Role) {
            first_role = i;
        }
        i += 1;
    }
    let needs_system = if (first_role < 0) {
        true
    } else if (first_role > 0) {
        true
    } else {
        parts[0].trim() != ""
    };

    if (needs_system) {
        let system = root.Role { name: "system", metadata: {} };
        let llm_parts: string[] = [""];
        for (let part in parts) {
            llm_parts.push(part);
        }
        let llm_values: unknown[] = [system];
        for (let value in values) {
            llm_values.push(value);
        }
        assemble_prompt(llm_parts, llm_values)
    } else {
        assemble_prompt(parts, values)
    }
}
