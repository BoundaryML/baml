// Internal helpers. Nothing here is part of the public ai surface.

/// (internal) One JSON Schema object for a function's parameters, derived from the
/// signature: every parameter typed via baml.json.schema.
///
/// `baml.json.schema` puts the classes and aliases a type reaches in `$defs`
/// at the root of the schema it returns, and its `$ref`s point there
/// (`#/$defs/Name`). A parameter's schema is a property of this object, so
/// its `$defs` move to this object's root, where those `$ref`s resolve. A
/// definition is keyed by its declaration's name, so two parameters that
/// reach one declaration contribute the same entry.
function _schema_for_signature(handler: reflect.AnyFunction) -> map<string, unknown> {
    let sig = reflect.signature(handler);
    let props: map<string, unknown> = {};
    let required: string[] = [];
    let definitions: map<string, unknown> = {};
    for (let a in sig.args) {
        let lowered = baml.json.schema(a.type);
        let property = baml.json.from_json<map<string, unknown>>(lowered);
        let reached = baml.json.path_or<map<string, unknown>>(lowered, ".$defs", {});
        for (let name in reached.keys()) {
            let _ = definitions.set(name, reached.get(name));
        }
        let _ = property.delete("$defs");
        let _ = props.set(a.name, property);
        required.push(a.name);
    }
    let schema: map<string, unknown> = { "type": "object", "properties": props, "required": required, "additionalProperties": false };
    if (definitions.length() > 0) {
        let _ = schema.set("$defs", definitions);
    }
    schema
}

/// Bound as `ctx` inside a compiler-desugared `@spec` prompt template.
/// `output_format` is the one placeholder the BEP allows.
class SpecCtx {
    _output_format: root.OutputFormat,

    function output_format(
        self,
        prefix: string | null | OutputFormatDefault = OutputFormatDefault.Auto,
        or_splitter: string | OutputFormatDefault = OutputFormatDefault.Auto,
        enum_value_prefix: string | null | OutputFormatDefault = OutputFormatDefault.Auto,
        hoisted_class_prefix: string | null | OutputFormatDefault = OutputFormatDefault.Auto,
        always_hoist_enums: bool | OutputFormatDefault = OutputFormatDefault.Auto,
        quote_class_fields: bool | OutputFormatDefault = OutputFormatDefault.Auto,
        hoist_classes: bool | "auto" | string[] | OutputFormatDefault = OutputFormatDefault.Auto,
        map_style: "angle" | "object" | OutputFormatDefault = OutputFormatDefault.Auto,
        render_null_as: string | null | OutputFormatDefault = OutputFormatDefault.Auto,
    ) -> string throws never {
        self._output_format._render(
            prefix = prefix,
            or_splitter = or_splitter,
            enum_value_prefix = enum_value_prefix,
            hoisted_class_prefix = hoisted_class_prefix,
            always_hoist_enums = always_hoist_enums,
            quote_class_fields = quote_class_fields,
            hoist_classes = hoist_classes,
            map_style = map_style,
            render_null_as = render_null_as,
        )
    }
}

/// Assemble a structural `ai.Prompt` from a tagged template's flattened
/// `parts` and `values`. `ai.Role` values start new messages; media
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
        // The reserved metadata key marks this role as assembler-inserted; the
        // VM strips it and skips legacy edge-trimming for the message.
        let system = root.Role { name: "system", metadata: { "__baml_synthetic_role": "1" } };
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

/// All input tokens, for a provider whose own input count (`uncached`)
/// excludes cache reads and writes: null when it reported no input count.
function total_input_tokens(uncached: int?, cached: int?, cache_write: int?) -> int? throws never {
    match (uncached) {
        let n: int => n + (cached ?? 0) + (cache_write ?? 0),
        null => null,
    }
}

/// Input tokens billed at the full rate, for a provider whose `total` counts
/// the cached share (and, where it reports them, the cache writes) too: null
/// when it reported no total, never below zero.
function uncached_input_tokens(
    total: int?,
    cached: int?,
    cache_write: int? = null,
) -> int? throws never {
    match (total) {
        let t: int => {
            let rest = t - (cached ?? 0) - (cache_write ?? 0);
            if (rest < 0) {
                0
            } else {
                rest
            }
        },
        null => null,
    }
}
