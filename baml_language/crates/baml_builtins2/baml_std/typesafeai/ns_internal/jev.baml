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

// One image sent in the request's `assets` map. JEV substitutes the image
// wherever `ref` appears in the state or instructions text.
class Asset {
    ref: string,
    format: string,
    data_base64: string,
}

class Plan {
    state: string,
    leaves: Leaf[],
    assets: Asset[],
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
            throw reject("JEV could not reflect the literal enum variant")
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
            _ => throw reject("JEV cannot format this literal value"),
        };
        return [ChoiceOption { key: key, description: null, value: value }];
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
            if (field.meta.skip) {
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

// JEV matches asset references as plain text anywhere in the request. The
// closing `>` keeps `<baml-image-1>` from being a prefix of `<baml-image-10>`.
function image_ref(index: int) -> string {
    `<baml-image-${index}>`
}

function prompt_text(text: string) -> string {
    if (text.includes("<baml-image-")) {
        throw reject("JEV prompt text cannot contain `<baml-image-`, which marks image references")
    }
    text
}

function image_format(mime: string) -> string {
    match (mime) {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        _ => throw reject(`JEV accepts PNG or JPEG images, not "${mime}"`),
    }
}

// Returns the reference that stands for `media` in text. An image that
// appears more than once is uploaded once and referenced at each place.
function attach(media: ai.content.Media, assets: Asset[], preview: bool) -> string {
    let resolved = match (media.media) {
        let image: baml.media.Image => {
            if (preview) {
                ai.wire.resolve_media_preview(image)
            } else {
                ai.wire.resolve_media(image, true)
            }
        },
        _ => throw reject(`JEV does not support ${media.kind()} input`),
    };
    let data: string = resolved.base64 else {
        if (preview) {
            throw ai.errors.PreviewUnsupported {
                provider: "typesafeai",
                detail: `JEV request preview cannot download image URL ${resolved.url ?? ""}`,
            }
        }
        throw reject(`JEV cannot fetch image URL ${resolved.url ?? ""}`)
    };
    for (let asset in assets) {
        if (asset.data_base64 == data) {
            return asset.ref;
        }
    }
    let ref = image_ref(assets.length());
    assets.push(Asset { ref: ref, format: image_format(resolved.mime_type), data_base64: data });
    ref
}

// Prompt rendering trims the text around a media part, so a space separates
// each reference from the text next to it.
function lower(blocks: ai.content.Block[], assets: Asset[], preview: bool) -> string {
    let out = "";
    let after_media = false;
    for (let block in blocks) {
        match (block) {
            let text: ai.content.Text => {
                let piece = prompt_text(text.text);
                if (after_media && piece.length() > 0) {
                    out = `${out} `;
                }
                out = `${out}${piece}`;
                after_media = false;
                null
            },
            let media: ai.content.Media => {
                if (out.length() > 0) {
                    out = `${out} `;
                }
                out = `${out}${attach(media, assets, preview)}`;
                after_media = true;
                null
            },
        };
    }
    out
}

function plan(input: ai.ModelTurnInput, preview: bool) -> Plan {
    if (!input.toolbox.is_empty()) {
        throw reject("JEV does not support tools")
    }
    let rendered = input.prompt(ai.internal.build_output_format(input.output_type));
    let prefix: ai.content.Block[] = [];
    let state: string[] = [];
    let assets: Asset[] = [];
    for (let message in rendered.messages()) {
        let text = lower(ai.wire.prompt_blocks(message), assets, preview);
        // An image here is repeated in every question's instructions, and
        // JEV bills each copy.
        if (message.role == "instructions") {
            prefix.push(ai.content.Text { text: text });
            prefix.push(ai.content.Text { text: "\n\n" });
        } else {
            if (text.trim().length() > 0) {
                state.push(text);
            }
        }
    }
    // Preserve prompt and journal order, including prior assistant responses.
    for (let entry in input.journal.entries()) {
        match (entry) {
            let user: ai.events.UserMessage => {
                state.push(lower(user.content, assets, preview));
                null
            },
            let assistant: ai.events.AssistantMessage => {
                for (let block in assistant.content) {
                    match (block) {
                        let text: ai.content.Text => {
                            state.push(prompt_text(text.text));
                            null
                        },
                        let media: ai.content.Media => {
                            state.push(attach(media, assets, preview));
                            null
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
        assets: assets,
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
    let body: map<string, baml.json.json> = { "model": c.model, "state": p.state, "questions": questions };
    if (p.assets.length() > 0) {
        let assets: map<string, baml.json.json> = {};
        for (let asset in p.assets) {
            let _ = assets.set(
                asset.ref,
                { "type": "image", "format": asset.format, "data_base64": asset.data_base64 },
            );
        }
        let _ = body.set("assets", assets);
        // TypeSafe keeps image input experimental and rejects it without this.
        let _ = headers.set("typesafe-danger-zone-images", "true");
    }
    let base = ai.wire.resolve_credential(c.base_url, null) ?? "https://api.typesafe.ai/v1";
    baml.http.Request {
        method: "POST",
        url: `${base}/systemone`,
        headers: headers,
        body: baml.json.stringify(body),
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
        // literal with the same JSON value. This is an accepted JEV limitation.
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

function invoke(c: root.Client, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let p = plan(input, false);
    // No streaming here: only the two transport deadlines apply.
    let sent = ai.wire.send_as_with_wire<Response>(
        ai.LlmTimeoutOptions._resolve(c.timeout)._apply(request(c, input, p, false)),
        "typesafeai",
        c.id(),
        capture_wire = c.capture_wire,
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
        model: c.model,
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
