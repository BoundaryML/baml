// Drex speaks the System One protocol, so request planning mirrors the
// TypeSafe JEV client: reflection owns traversal, metadata precedence,
// routing, and validation; reflected enums and classes are constructed
// through `let type` + JSON. Drex differs on the wire in two ways handled
// here: image evidence is a top-level `images` array referenced from the
// state as "Image N", and a request carries at most 16 questions.
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
    state: string,
    images: ai.content.Media[],
    leaves: Leaf[],
}

function reject(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "naceai",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

// Only finite Choice members are accepted. Reflected (canonical) union order
// is preserved; a key collision belongs to the first member that offered it.
function options(ty: reflect.Type) -> ChoiceOption[] {
    if let en: reflect.Enum = ty.as_enum() {
        let type T = ty;
        let out: ChoiceOption[] = [];
        for (let variant in en.values()) {
            if (!variant.meta.skip) {
                out.push(
                    ChoiceOption {
                        key: variant.meta.alias ?? variant.name,
                        description: variant.meta.description,
                        value: baml.json.decode<T>(baml.json.stringify(variant.name)),
                    },
                );
            }
        }
        return out;
    }
    if let union: reflect.Union = ty.as_union() {
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
    if let literal: reflect.Literal = ty.as_literal() {
        let value = literal.value();
        if let en: reflect.Enum = reflect.Type.of_value(value).as_enum() {
            let name = reflect.Enum.get_value(value);
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
            throw reject("Drex could not reflect the literal enum variant")
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
            _ => throw reject("Drex cannot format this literal value"),
        };
        return [ChoiceOption { key: key, description: null, value: value }];
    }
    throw reject(`Drex cannot convert Choice member ${ty.to_string()}`)
}

function accepts_null(ty: reflect.Type) -> bool {
    if (ty == reflect.Type.of<null>() || ty == reflect.Type.of<unknown>()) {
        return true;
    }
    if let union: reflect.Union = ty.as_union() {
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
            throw reject(`Drex cannot convert recursive type ${ty.to_string()}`)
        }
    }
    if let cls: reflect.Class = ty.as_class() {
        let out: Leaf[] = [];
        for (let field in cls.fields()) {
            let field_type = field.type;
            if (field.meta.skip) {
                if (!accepts_null(field_type)) {
                    throw reject(`Skipped Drex field ${field.name} must accept null`)
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
            throw reject("Drex cannot convert an empty class")
        }
        return out;
    }

    let choices: ChoiceOption[] = [];
    if (ty != bool.type() && ty != float.type()) {
        let seen: map<string, bool> = {};
        for (let option in options(ty)) {
            if (!seen.has(option.key)) {
                let _ = seen.set(option.key, true);
                choices.push(option);
            }
        }
        if (choices.length() < 2 || choices.length() > 255) {
            throw reject("Drex Choice requires between 2 and 255 distinct options")
        }
    }

    let type_description = match (ty.as_enum()) {
        let en: reflect.Enum => en.metadata().description,
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
        throw reject(`Drex question ${ids.join(".")} needs instructions: use role("instructions") or @description`)
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

/// Instructions are text only: an image there has no "Image N" slot.
function text_blocks(blocks: ai.content.Block[]) -> string {
    for (let block in blocks) {
        if let media: ai.content.Media = block {
            throw reject(`Drex does not support ${media.kind()} in instructions`)
        }
    }
    ai.content.text(blocks)
}

/// Evidence collected in order: state paragraphs and the images they cite.
class Evidence {
    state: string[],
    images: ai.content.Media[],
}

/// Absorb one message's blocks. Text is kept verbatim; an image joins the
/// request's `images` array and is cited in the text as "Image N" (1-based,
/// request order), which is how Drex expects images to be referred to.
/// Every other media kind is rejected before HTTP.
function absorb(evidence: Evidence, blocks: ai.content.Block[]) -> Evidence {
    let images = evidence.images;
    let out = "";
    for (let block in blocks) {
        match (block) {
            let text: ai.content.Text => {
                out = `${out}${text.text}`;
                null
            },
            let media: ai.content.Media => {
                if (media.kind() != "image") {
                    throw reject(`Drex does not support ${media.kind()} input`)
                }
                images = images.concat([media]);
                let marker = `Image ${images.length()}`;
                out = if (out.length() == 0 || out.ends_with(" ") || out.ends_with("\n")) {
                    `${out}${marker}`
                } else {
                    `${out} ${marker}`
                };
                null
            },
        };
    }
    let text = out.trim();
    Evidence {
        state: if (text.length() == 0) {
            evidence.state
        } else {
            evidence.state.concat([text])
        },
        images: images,
    }
}

function plan(input: ai.ModelTurnInput) -> Plan {
    if (!input.toolbox.is_empty()) {
        throw reject("Drex does not support tools")
    }
    let rendered = input.prompt(ai.internal.build_output_format(input.output_type));
    let prefix: ai.content.Block[] = [];
    let evidence = Evidence { state: [], images: [] };
    for (let message in rendered.messages()) {
        let blocks = ai.wire.prompt_blocks(message);
        if (message.role == "instructions") {
            prefix.push(ai.content.Text { text: text_blocks(blocks) });
            prefix.push(ai.content.Text { text: "\n\n" });
        } else {
            evidence = absorb(evidence, blocks);
        }
    }
    // Drex takes one shared state. Preserve prompt and journal order,
    // including prior assistant responses, as ordered evidence.
    for (let entry in input.journal.entries()) {
        match (entry) {
            let user: ai.events.UserMessage => {
                evidence = absorb(evidence, user.content);
                null
            },
            let assistant: ai.events.AssistantMessage => {
                let blocks: ai.content.Block[] = [];
                for (let block in assistant.content) {
                    match (block) {
                        let text: ai.content.Text => {
                            blocks.push(text);
                            null
                        },
                        let media: ai.content.Media => {
                            blocks.push(media);
                            null
                        },
                        let tool: ai.content.ToolUse => {
                            throw reject("Drex does not support tool history")
                        },
                        _ => null,
                    };
                }
                evidence = absorb(evidence, blocks);
                null
            },
            let tool: ai.events.ToolCompleted => throw reject("Drex does not support tool history"),
            _ => null,
        };
    }
    if (evidence.images.length() > 8) {
        throw reject("Drex accepts at most 8 images per request")
    }
    let leaves = collect(input.output_type, [], [], null, false, prefix, []);
    if (leaves.length() > 16) {
        throw reject(`Drex accepts at most 16 questions per request, this output needs ${leaves.length()}`)
    }
    Plan { state: evidence.state.join("\n\n"), images: evidence.images, leaves: leaves }
}

/// Inline image bytes as data URLs. A preview never downloads: a remote
/// image is reported as unsupported for preview, like other inline-only
/// providers. On invocation, remote images are fetched first.
function images(p: Plan, preview: bool) -> string[] {
    let out: string[] = [];
    for (let media in p.images) {
        let image: baml.media.Image = media.media else {
            throw reject("Drex only supports image media")
        };
        let resolved = if (preview) {
            ai.wire.resolve_media_preview(image)
        } else {
            ai.wire.resolve_media(image, true)
        };
        if let url: string = resolved.url {
            if (preview) {
                throw ai.errors.PreviewUnsupported {
                    provider: "naceai",
                    detail: "Drex requires downloaded image bytes during request preview",
                }
            }
            throw reject(`Drex cannot inline image ${url}`)
        }
        out.push(`data:${resolved.mime_type};base64,${resolved.base64 ?? ""}`);
    }
    out
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
    let body: map<string, baml.json.json> = { "model": c.model, "state": p.state, "questions": questions };
    // Models other than drex-max reject a non-empty images array, so the key
    // is only present when there is something to send.
    if (p.images.length() > 0) {
        let _ = body.set("images", images(p, preview));
    }
    let headers: map<string, string> = { "content-type": "application/json" };
    if (!preview) {
        let key: string = ai.wire.resolve_credential(c.api_key, "NACE_API_KEY") else {
            throw reject("naceai needs api_key or NACE_API_KEY")
        };
        let _ = headers.set("authorization", `Bearer ${key}`);
    }
    let base = ai.wire.resolve_credential(c.base_url, null) ?? "https://console.nace.ai/v1";
    let trimmed = if (base.ends_with("/")) {
        base.slice(0, base.length() - 1)
    } else {
        base
    };
    baml.http.Request {
        method: "POST",
        url: `${trimmed}/systemone`,
        headers: headers,
        body: baml.json.stringify(body),
    }
}

// Only the fields this client reads. Drex also returns `confidence`,
// `probabilities`, and (for score questions, which this client never asks)
// a `legend` array; unknown fields are ignored by the decoder.
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
    model: string?,
    answers: map<string, Answer>,
    usage: Usage?,
}

function failed(detail: string) -> ai.errors.ParseFailed {
    ai.errors.ParseFailed { provider: "naceai", raw_output: detail }
}

function decode(leaf: Leaf, answer: Answer) -> unknown {
    if (leaf.options.length() == 0) {
        let probability: float = answer.noul else {
            throw failed(`Missing noul for ${leaf.id}`)
        };
        if (answer.type != "noul" || probability < 0.0 || probability > 1.0) {
            throw failed(`Invalid noul answer for ${leaf.id}`)
        }
        if (leaf.ty == bool.type()) {
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
    if let cls: reflect.Class = ty.as_class() {
        let fields: map<string, unknown> = {};
        for (let field in cls.fields()) {
            if (field.meta.skip) {
                let _ = fields.set(field.name, null);
            } else {
                let field_type = field.type;
                let _ = fields.set(field.name, assemble(field_type, path.concat([field.name]), values));
            }
        }
        // JSON currently cannot preserve an already-selected enum versus string
        // literal with the same JSON value. This matches the TypeSafe adapter limitation.
        let type T = ty;
        return baml.json.decode<T>(baml.json.stringify(fields));
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
    // No streaming here: only the two transport deadlines apply.
    let sent = ai.wire.send_as_with_wire<Response>(
        ai.LlmTimeoutOptions._resolve(c.timeout)._apply(request(c, input, p, false)),
        ai.events.ModelKey { provider: c.provider(), model: c.model },
        capture_wire = c.capture_wire,
    );
    let values: map<string, unknown> = {};
    for (let leaf in p.leaves) {
        let answer: Answer = sent.value.answers.get(leaf.id) else {
            throw failed(`Missing Drex answer ${leaf.id}`)
        };
        let _ = values.set(leaf.id, decode(leaf, answer));
    }
    let value = assemble(input.output_type, [], values);
    let usage: ai.events.Usage? = match (sent.value.usage) {
        let reported: Usage => {
            ai.events.Usage {
                input_tokens: reported.input_tokens,
                output_tokens: reported.output_tokens,
                cached_input_tokens: null,
                reasoning_tokens: null,
                uncached_input_tokens: reported.input_tokens,
            }
        },
        null => null,
    };
    ai.internal.with_own_call(
        ai.ModelTurn {
            content: [ai.content.Text { text: baml.json.stringify(value) }],
            parsed_output: ai.ParsedOutput { value: value },
            stop_reason: ai.content.StopReason.Complete,
            calls: [],
            child_turns: [],
        },
        sent.call,
        usage,
        c.model,
        sent.value.model,
    )
}
