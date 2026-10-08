// Reflection owns traversal, metadata precedence, routing, and validation.
// Reflected enums and classes are constructed through unreflect + JSON.
class ChoiceOption {
    key: string,
    description: string?,
    value: unknown,
}

class Leaf {
    id: string,
    ty: reflect.Type,
    instructions: string,
    options: ChoiceOption[],
}

class Plan {
    evidence: ai.content.Block[],
    leaves: Leaf[],
}

function reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "openai-decisions",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// Only finite Choice members are accepted. Reflected (canonical) union order
// is preserved; a key collision belongs to the first member that offered it.
function options(ty: reflect.Type) -> ChoiceOption[] {
    if let en: reflect.enum.Type = ty.as_enum() {
        type T = unreflect(ty);
        let out: ChoiceOption[] = [];
        for (let variant in en.values()) {
            if (!variant.meta.skip) {
                out.push(
                    ChoiceOption {
                        key: variant.meta.alias ?? variant.name,
                        description: variant.meta.description,
                        value: baml.json.from_string<T>(baml.json.stringify(variant.name)),
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
    if (ty == reflect.Type.of<null>()) {
        return [ChoiceOption { key: "<null>", description: null, value: null }];
    }
    if let literal: reflect.literal.Type = ty.as_literal() {
        let value = literal.value();
        if let en: reflect.enum.Type = reflect.Type.of_value(value).as_enum() {
            let name = reflect.enum.get_value(value);
            for (let variant in en.values()) {
                if (variant.name == name) {
                    if (variant.meta.skip) {
                        return [];
                    }
                    return [
                        ChoiceOption {
                            key: variant.meta.alias ?? variant.name,
                            description: variant.meta.description,
                            value: value,
                        },
                    ];
                }
            }
            throw reject("Decisions could not reflect the literal enum variant")
        }
        let key = match (value) {
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
            _ => throw reject("Decisions cannot format this literal value"),
        };
        return [ChoiceOption { key: key, description: null, value: value }];
    }
    throw reject(`Decisions cannot convert Choice member ${ty.to_string()}`)
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
            throw reject(`Decisions cannot convert recursive type ${ty.to_string()}`)
        }
    }
    if let cls: reflect.class.Type = ty.as_class() {
        let out: Leaf[] = [];
        for (let field in cls.fields()) {
            let field_type: reflect.Type = field.type else {
                throw reject(`Decisions cannot reflect field ${field.name}`)
            };
            if (field.meta.skip) {
                if (!accepts_null(field_type)) {
                    throw reject(`Skipped Decisions field ${field.name} must accept null`)
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
            throw reject("Decisions cannot convert an empty class")
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
            throw reject("Decisions Choice requires between 2 and 255 distinct options")
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
        throw reject(`Decisions question ${ids.join(".")} needs instructions: use role("instructions") or @description`)
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

// Validate before any credential resolution or media download.
function validate_blocks(blocks: ai.content.Block[], instructions: bool = false) -> void {
    for (let block in blocks) {
        if let media: ai.content.Media = block {
            if (instructions || media.kind() != "image") {
                throw reject(`Decisions does not support ${media.kind()} in this input`)
            }
        }
    }
}

function plan(input: ai.ModelTurnInput) -> Plan {
    if (!input.toolbox.is_empty()) {
        throw reject("Decisions does not support tools")
    }
    let rendered = input.prompt(ai.internal.build_output_format(input.output_type));
    let prefix: ai.content.Block[] = [];
    let evidence: ai.content.Block[] = [];
    for (let message in rendered.messages()) {
        let blocks = ai.wire.prompt_blocks(message);
        validate_blocks(blocks, instructions = message.role == "instructions");
        if (message.role == "instructions") {
            prefix.push(ai.content.Text { text: ai.content.text(blocks) });
            prefix.push(ai.content.Text { text: "\n\n" });
        } else {
            if (evidence.length() > 0) {
                evidence.push(ai.content.Text { text: "\n\n" });
            }
            evidence = evidence.concat(blocks);
        }
    }
    // The endpoint only accepts user messages. Like TypeSafe, treat prior
    // assistant text as ordered evidence, rather than a chat conversation.
    for (let entry in input.journal.entries()) {
        let blocks: ai.content.Block[] = match (entry) {
            let user: ai.events.UserMessage => user.content,
            let assistant: ai.events.AssistantMessage => {
                let out: ai.content.Block[] = [];
                for (let block in assistant.content) {
                    match (block) {
                        let text: ai.content.Text => out.push(text),
                        let media: ai.content.Media => out.push(media),
                        let tool: ai.content.ToolUse => {
                            throw reject("Decisions does not support tool history")
                        },
                        _ => null,
                    };
                }
                out
            },
            let tool: ai.events.ToolCompleted => {
                throw reject("Decisions does not support tool history")
            },
            _ => [],
        };
        validate_blocks(blocks);
        if (blocks.length() > 0) {
            if (evidence.length() > 0) {
                evidence.push(ai.content.Text { text: "\n\n" });
            }
            evidence = evidence.concat(blocks);
        }
    }
    let images = 0;
    for (let block in evidence) {
        if let media: ai.content.Media = block {
            images = images + 1;
        }
    }
    if (images > 128) {
        throw reject("Decisions supports at most 128 images per request")
    }
    Plan { evidence: evidence, leaves: collect(input.output_type, [], [], null, false, prefix, []) }
}

function evidence(p: Plan, preview: bool) -> baml.json.json {
    let parts: baml.json.json[] = [];
    let has_images = false;
    for (let block in p.evidence) {
        match (block) {
            let text: ai.content.Text => {
                parts.push({ "type": "input_text", "text": text.text });
            },
            let media: ai.content.Media => {
                let image: baml.media.Image = media.media else {
                    throw reject("Decisions only supports image media")
                };
                let resolved = if (preview) {
                    ai.wire.resolve_media_preview(image)
                } else {
                    ai.wire.resolve_media(image, true)
                };
                if (resolved.url != null) {
                    throw ai.errors.PreviewUnsupported {
                        provider: "openai-decisions",
                        detail: "Decisions requires downloaded image bytes during request preview",
                    }
                }
                parts.push(
                    {
                        "type": "input_image",
                        "image_url": `data:${resolved.mime_type};base64,${resolved.base64 ?? ""}`,
                    },
                );
                has_images = true;
            },
        };
    }
    if (!has_images) {
        return ai.content.text(p.evidence);
    }
    return [{ "role": "user", "content": parts }];
}

function request(
    c: root.DecisionsClient,
    input: ai.ModelTurnInput,
    p: Plan,
    preview: bool,
) -> baml.http.Request {
    let questions: baml.json.json[] = [];
    for (let leaf in p.leaves) {
        let question: map<string, baml.json.json> = {
            "type": if (leaf.options.length() == 0) {
                "predicate"
            } else {
                "choice"
            },
            "name": leaf.id,
            "instructions": leaf.instructions,
        };
        if (leaf.options.length() > 0) {
            let choices: baml.json.json[] = [];
            for (let option in leaf.options) {
                let choice: map<string, baml.json.json> = { "value": option.key };
                if let description: string = option.description {
                    let _ = choice.set("description", description);
                }
                choices.push(choice);
            }
            let _ = question.set("choices", choices);
        }
        questions.push(question);
    }
    let headers: map<string, string> = { "content-type": "application/json" };
    if (!preview) {
        let key: string = ai.wire.resolve_credential(c.api_key, "OPENAI_API_KEY") else {
            throw reject("openai-decisions needs api_key or OPENAI_API_KEY")
        };
        let _ = headers.set("authorization", `Bearer ${key}`);
    }
    let base = ai.wire.resolve_credential(c.base_url, null) ?? "https://api.openai.com/v1";
    baml.http.Request {
        method: "POST",
        url: `${base}/decisions`,
        headers: headers,
        body: baml.json.stringify(
            { "model": c.model, "input": evidence(p, preview), "questions": questions },
        ),
    }
}

class Answer {
    type: string,
    name: string?,
    probability: float?,
    choice: string?,
}

class Usage {
    input_tokens: int,
    output_tokens: int,
}

class Response {
    answers: Answer[],
    model: string?,
    usage: Usage?,
}

function failed(detail: string) -> ai.errors.ParseFailed {
    ai.errors.ParseFailed { provider: "openai-decisions", raw_output: detail }
}

function decode(leaf: Leaf, answer: Answer) -> unknown {
    if (answer.type == "refusal") {
        throw ai.errors.Refused { provider: "openai-decisions", reason: `Decisions refused question ${leaf.id}`, raw_body: null }
    }
    if (leaf.options.length() == 0) {
        let probability: float = answer.probability else {
            throw failed(`Missing predicate for ${leaf.id}`)
        };
        if (answer.type != "predicate" || probability < 0.0 || probability > 1.0) {
            throw failed(`Invalid predicate answer for ${leaf.id}`)
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
        let fields: map<string, unknown> = {};
        for (let field in cls.fields()) {
            if (field.meta.skip) {
                let _ = fields.set(field.name, null);
            } else {
                let field_type: reflect.Type = field.type else {
                    throw failed("Missing field type")
                };
                let _ = fields.set(field.name, assemble(field_type, path.concat([field.name]), values));
            }
        }
        // JSON currently cannot preserve an already-selected enum versus string
        // literal with the same JSON value. This matches the TypeSafe adapter limitation.
        type T = unreflect(ty);
        return baml.json.from_string<T>(baml.json.to_string(fields));
    }
    values.get(
        if (path.length() == 0) {
            "result"
        } else {
            path.join(".")
        },
    )
}

function invoke(c: root.DecisionsClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let p = plan(input);
    // No streaming here: only the two transport deadlines apply.
    let sent = ai.wire.send_as_with_wire<Response>(
        ai.LlmTimeoutOptions._resolve(c.timeout)._apply(request(c, input, p, false)),
        "openai-decisions",
        c.id(),
        capture_wire = c.capture_wire,
    );
    if (sent.value.answers.length() != p.leaves.length()) {
        throw failed("Decisions returned a different number of answers than questions")
    }
    let answers: map<string, Answer> = {};
    for (let answer in sent.value.answers) {
        let name: string = answer.name else {
            throw failed("Decisions answer is missing its name")
        };
        if (answers.has(name)) {
            throw failed(`Duplicate Decisions answer ${name}`)
        }
        let _ = answers.set(name, answer);
    }
    let values: map<string, unknown> = {};
    for (let leaf in p.leaves) {
        let answer: Answer = answers.get(leaf.id) else {
            throw failed(`Missing Decisions answer ${leaf.id}`)
        };
        let _ = values.set(leaf.id, decode(leaf, answer));
    }
    let value = assemble(input.output_type, [], values);
    ai.ModelTurn {
        model: sent.value.model ?? c.model,
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
                    uncached_input_tokens: usage.input_tokens,
                }
            },
            null => null,
        },
    }
}
