// Reflection owns traversal, metadata precedence, routing, and validation.
// Native helpers only bridge operations reflection cannot express yet.
class ChoiceOption {
    key: string,
    description: string?,
    value: unknown,
    skipped: bool,
}

/// (internal) Extract a literal, enum-variant, or null value without parsing
/// the type's display text. All Choice formatting and metadata stay in BAML.
//baml:mut_vm
function constant_value(ty: reflect.Type) -> unknown throws baml.errors.InvalidArgument {
    $rust_function
}

function constant(ty: reflect.Type) -> ChoiceOption {
    let value = constant_value(ty);
    if let en: reflect.enum.Type = reflect.Type.of_value(value).as_enum() {
        let name = reflect.enum.get_value(value);
        for (let variant in en.values()) {
            if (variant.name == name) {
                return ChoiceOption {
                    key: variant.meta.alias ?? variant.name,
                    description: variant.meta.description,
                    value: value,
                    skipped: skipped(en.as_type(), name),
                };
            }
        }
        throw reject("JEV could not reflect the literal enum variant")
    }
    let key = match (value) {
        null => "<null>",
        let text: string => text,
        let number: int => number.to_string(),
        let number: bigint => number.to_string(),
        let boolean: bool => {
            if (boolean) {
                "true"
            } else {
                "false"
            }
        },
        _ => throw reject("JEV cannot format this literal value"),
    };
    ChoiceOption { key: key, description: null, value: value, skipped: false }
}

/// (internal) Construct the named variant of an already reflected enum.
//baml:mut_vm
function enum_value(ty: reflect.Type, name: string) -> unknown throws baml.errors.InvalidArgument {
    $rust_function
}

/// (internal) Reflection metadata does not yet expose SAP's skip flag.
//baml:vm
function skipped(ty: reflect.Type, name: string) -> bool throws baml.errors.InvalidArgument {
    $rust_function
}

/// (internal) Construct a realized class, preserving its generic arguments.
/// An unreflect + JSON round trip can reconstruct classes, but cannot preserve
/// an already-selected enum versus string literal with the same JSON value.
//baml:mut_vm
function class_value(
    ty: reflect.Type,
    fields: unknown[],
) -> unknown throws baml.errors.InvalidArgument {
    $rust_function
}

class Leaf {
    id: string,
    ty: reflect.Type,
    instructions: string,
    options: ChoiceOption[],
}

class Plan {
    state: string,
    leaves: Leaf[],
}

function reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "typesafeai",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// Only finite Choice members are accepted. Reflected (canonical) union order
// is preserved; a key collision belongs to the first member that offered it.
function options(ty: reflect.Type) -> ChoiceOption[] {
    if let en: reflect.enum.Type = ty.as_enum() {
        let out: ChoiceOption[] = [];
        for (let variant in en.values()) {
            if (!skipped(ty, variant.name)) {
                out.push(
                    ChoiceOption {
                        key: variant.meta.alias ?? variant.name,
                        description: variant.meta.description,
                        value: enum_value(ty, variant.name),
                        skipped: false,
                    },
                );
            }
        }
        return out;
    }
    if let union: reflect.union.Type = ty.as_union() {
        let out: ChoiceOption[] = [];
        for (let member in union.member_types()) {
            for (let option in options(member)) {
                out.push(option);
            }
        }
        return out;
    }
    if (ty.as_literal() != null || ty == reflect.Type.of<null>()) {
        let option = constant(ty);
        if (option.skipped) {
            return [];
        }
        return [option];
    }
    throw reject(`JEV cannot convert Choice member ${ty.to_string()}`)
}

function accepts_null(ty: reflect.Type) -> bool {
    if (ty == reflect.Type.of<null>() || ty == reflect.Type.of<unknown>()) {
        return true;
    }
    if let union: reflect.union.Type = ty.as_union() {
        for (let member in union.member_types()) {
            if (accepts_null(member)) {
                return true;
            }
        }
    }
    false
}

function collect(
    ty: reflect.Type,
    ids: string[],
    labels: string[],
    description: string?,
    aliased: bool,
    prefix: ai.content.Block[],
    ancestors: reflect.Type[],
) -> Leaf[] {
    for (let ancestor in ancestors) {
        if (ancestor == ty) {
            throw reject(`JEV cannot convert recursive type ${ty.to_string()}`)
        }
    }
    if let cls: reflect.class.Type = ty.as_class() {
        let out: Leaf[] = [];
        for (let field in cls.fields()) {
            let field_type: reflect.Type = field.type else {
                throw reject(`JEV cannot reflect field ${field.name}`)
            };
            if (skipped(ty, field.name)) {
                if (!accepts_null(field_type)) {
                    throw reject(`Skipped JEV field ${field.name} must accept null`)
                }
            } else {
                for (
                    let leaf in collect(
                        field_type,
                        ids.concat([field.name]),
                        labels.concat([field.meta.alias ?? field.name]),
                        field.meta.description,
                        field.meta.alias != null,
                        prefix,
                        ancestors.concat([ty]),
                    )
                ) {
                    out.push(leaf);
                }
            }
        }
        if (out.length() == 0) {
            throw reject("JEV cannot convert an empty class")
        }
        return out;
    }

    let choices: ChoiceOption[] = [];
    if (ty != reflect.Type.of<bool>() && ty != reflect.Type.of<float>()) {
        let seen: map<string, bool> = {};
        for (let option in options(ty)) {
            if (!seen.has(option.key)) {
                let _ = seen.set(option.key, true);
                choices.push(option);
            }
        }
        if (choices.length() < 2 || choices.length() > 255) {
            throw reject("JEV Choice requires between 2 and 255 distinct options")
        }
    }

    let type_description = match (ty.as_enum()) {
        let en: reflect.enum.Type => en.meta().description,
        null => null,
    };
    let local = description ?? type_description ?? "";
    let contextual = if (ids.length() > 1 || aliased) {
        `${labels.join(".")}: ${local}`
    } else {
        local
    };
    let parts = prefix.concat([ai.content.Text { text: contextual }]);
    let instructions = ai.content.text(parts).trim();
    if (instructions.length() == 0) {
        throw reject(`JEV question ${ids.join(".")} needs instructions: use role("instructions") or @description`)
    }
    return [
        Leaf {
            id: if (ids.length() == 0) {
                "result"
            } else {
                ids.join(".")
            },
            ty: ty,
            instructions: instructions,
            options: choices,
        },
    ];
}

function text_blocks(blocks: ai.content.Block[]) -> string {
    for (let block in blocks) {
        if let media: ai.content.Media = block {
            throw reject(`JEV does not support ${media.kind()} input`)
        }
    }
    ai.content.text(blocks)
}

function plan(input: ai.ModelTurnInput) -> Plan {
    if (!input.toolbox.is_empty()) {
        throw reject("JEV does not support tools")
    }
    let rendered = input.prompt(ai.internal.build_output_format(input.output_type));
    let prefix: ai.content.Block[] = [];
    let state: string[] = [];
    for (let message in rendered.messages()) {
        let text = text_blocks(ai.wire.prompt_blocks(message));
        if (message.role == "instructions") {
            prefix.push(ai.content.Text { text: text });
            prefix.push(ai.content.Text { text: "\n\n" });
        } else {
            if (text.trim().length() > 0) {
                state.push(text);
            }
        }
    }
    // JEV accepts text state. Preserve prompt and journal order, including
    // prior assistant responses; reject media instead of hiding placeholders.
    for (let entry in input.journal.entries()) {
        match (entry) {
            let user: ai.events.UserMessage => {
                state.push(text_blocks(user.content));
                null
            },
            let assistant: ai.events.AssistantMessage => {
                for (let block in assistant.content) {
                    match (block) {
                        let text: ai.content.Text => {
                            state.push(text.text);
                            null
                        },
                        let media: ai.content.Media => {
                            throw reject("JEV does not support media in history")
                        },
                        let tool: ai.content.ToolUse => {
                            throw reject("JEV does not support tool history")
                        },
                        _ => null,
                    };
                }
            },
            let tool: ai.events.ToolCompleted => throw reject("JEV does not support tool history"),
            _ => null,
        };
    }
    Plan {
        state: state.join("\n\n"),
        leaves: collect(input.output_type, [], [], null, false, prefix, []),
    }
}

function request(
    c: root.Client,
    input: ai.ModelTurnInput,
    p: Plan,
    preview: bool,
) -> baml.http.Request {
    let questions: map<string, baml.json.json> = {};
    for (let leaf in p.leaves) {
        let question: map<string, baml.json.json> = {
            "type": if (leaf.options.length() == 0) {
                "noul"
            } else {
                "choice"
            },
            "instructions": leaf.instructions,
        };
        if (leaf.options.length() > 0) {
            let criteria: map<string, baml.json.json> = {};
            for (let option in leaf.options) {
                let _ = criteria.set(option.key, option.description);
            }
            let _ = question.set("criteria", criteria);
        }
        let _ = questions.set(leaf.id, question);
    }
    let headers: map<string, string> = { "content-type": "application/json" };
    if (!preview) {
        let key: string = ai.wire.resolve_credential(c.api_key, "TYPESAFE_API_KEY") else {
            throw reject("typesafeai needs api_key or TYPESAFE_API_KEY")
        };
        let _ = headers.set("authorization", `Bearer ${key}`);
    }
    let base = ai.wire.resolve_credential(c.base_url, null) ?? "https://api.typesafe.ai/v1";
    baml.http.Request {
        method: "POST",
        url: `${base}/systemone`,
        headers: headers,
        body: baml.json.stringify({ "model": c.model, "state": p.state, "questions": questions }),
    }
}

class Answer {
    type: string,
    noul: float?,
    choice: string?,
}

class Usage {
    input_tokens: int,
    output_tokens: int,
}

class Response {
    answers: map<string, Answer>,
    usage: Usage?,
}

function failed(detail: string) -> ai.errors.ParseFailed {
    ai.errors.ParseFailed { provider: "typesafeai", raw_output: detail }
}

function decode(leaf: Leaf, answer: Answer) -> unknown {
    if (leaf.options.length() == 0) {
        let probability: float = answer.noul else {
            throw failed(`Missing noul for ${leaf.id}`)
        };
        if (answer.type != "noul" || probability < 0.0 || probability > 1.0) {
            throw failed(`Invalid noul answer for ${leaf.id}`)
        }
        if (leaf.ty == reflect.Type.of<bool>()) {
            return probability >= 0.5;
        }
        return probability;
    }
    if (answer.type != "choice") {
        throw failed(`Expected choice for ${leaf.id}`)
    }
    for (let option in leaf.options) {
        if (answer.choice == option.key) {
            return option.value;
        }
    }
    throw failed(`Unknown choice for ${leaf.id}: ${answer.choice ?? "<missing>"}`)
}

function assemble(ty: reflect.Type, path: string[], values: map<string, unknown>) -> unknown {
    if let cls: reflect.class.Type = ty.as_class() {
        let fields: unknown[] = [];
        for (let field in cls.fields()) {
            if (skipped(ty, field.name)) {
                fields.push(null);
            } else {
                let field_type: reflect.Type = field.type else {
                    throw failed("Missing field type")
                };
                fields.push(assemble(field_type, path.concat([field.name]), values));
            }
        }
        return class_value(ty, fields);
    }
    values.get(
        if (path.length() == 0) {
            "result"
        } else {
            path.join(".")
        },
    )
}

function invoke(c: root.Client, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let p = plan(input);
    let sent = ai.wire.send_as_with_wire<Response>(
        request(c, input, p, false),
        "typesafeai",
        c.id(),
        capture_wire = c.capture_wire,
        request_timeout_ms = c.request_timeout_ms,
    );
    let values: map<string, unknown> = {};
    for (let leaf in p.leaves) {
        let answer: Answer = sent.value.answers.get(leaf.id) else {
            throw failed(`Missing JEV answer ${leaf.id}`)
        };
        let _ = values.set(leaf.id, decode(leaf, answer));
    }
    let value = assemble(input.output_type, [], values);
    ai.ModelTurn {
        content: [ai.content.Text { text: baml.json.to_string(value) }],
        parsed_output: ai.ParsedOutput { value: value },
        stop_reason: ai.content.StopReason.Complete,
        calls: [sent.call],
        usage: match (sent.value.usage) {
            let usage: Usage => {
                ai.events.Usage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    cached_input_tokens: null,
                    reasoning_tokens: null,
                }
            },
            null => null,
        },
    }
}
