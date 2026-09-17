function _invalid(detail: string) -> ai.errors.InvalidRequest {
    ai.errors.InvalidRequest {
        provider: "typesafeai",
        status_code: null,
        detail: detail,
        raw_body: null,
    }
}

function _add_choice(
    criteria: map<string, baml.json.json>,
    key: string,
    description: baml.json.json,
    owner: string,
) -> void {
    if (criteria.keys().includes(key)) {
        throw _invalid(`${owner} has duplicate Jev choice ${key}`);
    }
    let _ = criteria.set(key, description);
}

function _add_enum_choices(
    criteria: map<string, baml.json.json>,
    enum_type: reflect.enum.Type,
    owner: string,
) -> void {
    for (let value in enum_type.values()) {
        let key = value.meta.alias ?? value.name;
        let description: baml.json.json = value.meta.description ?? value.meta.docstring;
        _add_choice(criteria, key, description, owner);
    }
}

function _question(c: root.Client, output: reflect.Type, instructions: string) -> root.Question {
    if (output == reflect.Type.of<bool>()) {
        if (c.noul_threshold < 0.0 || c.noul_threshold > 1.0) {
            throw _invalid("noul_threshold must be between 0 and 1");
        }
        return root.Question { type: "noul", instructions: instructions, criteria: null };
    }
    if let enum_type: reflect.enum.Type = output.as_enum() {
        let criteria: map<string, baml.json.json> = {};
        _add_enum_choices(criteria, enum_type, `enum ${output.to_string()}`);
        if (criteria.keys().length() < 2) {
            throw _invalid(`enum ${output.to_string()} needs at least two Jev choices`);
        }
        return root.Question { type: "choice", instructions: instructions, criteria: criteria };
    }
    if let union_type: reflect.union.Type = output.as_union() {
        let criteria: map<string, baml.json.json> = {};
        for (let member in union_type.member_types()) {
            if let member_enum: reflect.enum.Type = member.as_enum() {
                _add_enum_choices(criteria, member_enum, `union ${output.to_string()}`);
            } else if let literal: reflect.literal.Type = member.as_literal() {
                let key: string = literal.value() else {
                    throw _invalid(`union ${output.to_string()} must contain only string literals or enums`)
                };
                _add_choice(criteria, key, null, `union ${output.to_string()}`);
            } else {
                throw _invalid(`union ${output.to_string()} must contain only string literals or enums`);
            }
        }
        if (criteria.keys().length() < 2) {
            throw _invalid(`union ${output.to_string()} needs at least two Jev choices`);
        }
        return root.Question { type: "choice", instructions: instructions, criteria: criteria };
    }
    if (output == reflect.Type.of<float>()) {
        if let levels: string[] = c.score_criteria {
            if (levels.length() < 2) {
                throw _invalid("score_criteria must contain at least two levels");
            }
            return root.Question {
                type: "score",
                instructions: instructions,
                criteria: baml.json.from(levels),
            };
        }
        return root.Question { type: "noul", instructions: instructions, criteria: null };
    }
    throw _invalid(`TypeSafe AI cannot produce BAML output type ${output.to_string()}; use bool, float, an enum, a string-literal/enum union, or a class of those types`)
}

function _add_questions(
    c: root.Client,
    output: reflect.Type,
    instructions: string,
    path: string,
    depth: int,
    questions: map<string, root.Question>,
) -> void {
    if (depth >= 16) {
        throw _invalid("nested class depth exceeds 16 (or the class is recursive)");
    }
    if let class_type: reflect.class.Type = output.as_class() {
        let output_names: string[] = [];
        for (let field in class_type.fields()) {
            let field_type: reflect.Type = field.type else {
                throw _invalid(`class field ${field.name} has no reflected type`);
            };
            let output_name = field.meta.alias ?? field.name;
            if (output_names.includes(output_name)) {
                throw _invalid(`class ${output.to_string()} has duplicate output key ${output_name}`);
            }
            output_names.push(output_name);
            let key = if (path == "") {
                field.name
            } else {
                `${path}.${field.name}`
            };
            if (field_type.as_class() != null) {
                _add_questions(c, field_type, instructions, key, depth + 1, questions);
            } else {
                let field_instructions: string = field.meta.description ?? field.meta.docstring else {
                    throw _invalid(`class field ${key} needs @description or a doc comment with a complete question`);
                };
                let _ = questions.set(key, _question(c, field_type, field_instructions));
            }
        }
    } else {
        let _ = questions.set("result", _question(c, output, instructions));
    }
}

function _questions(
    c: root.Client,
    output: reflect.Type,
    instructions: string,
) -> map<string, root.Question> {
    let questions: map<string, root.Question> = {};
    _add_questions(c, output, instructions, "", 0, questions);
    if (questions.keys().length() == 0) {
        throw _invalid(`type ${output.to_string()} has no fields to evaluate`);
    }
    questions
}

function _state(input: ai.ModelTurnInput, client_id: string) -> baml.json.json {
    if (!input.toolbox.is_empty()) {
        throw _invalid("Jev does not support tool calls");
    }
    let prompt = input.prompt(ai.internal.build_output_format(input.output_type));
    let messages: baml.json.json[] = [];
    for (let message in prompt.messages()) {
        let parts: string[] = [];
        for (let part in message.parts) {
            match (part) {
                let text: string => parts.push(text),
                let media: ai.MediaPart => throw _invalid("Jev does not support media input"),
            };
        }
        let content = parts.join("").trim();
        if (content != "") {
            messages.push(
                {
                    "role": if (message.role == "") {
                        "user"
                    } else {
                        message.role
                    },
                    "content": content,
                },
            );
        }
    }
    let journal = ai.wire.sanitize_for_client(input.journal, client_id);
    for (let event in journal.entries()) {
        if let user: ai.events.UserMessage = event {
            let parts: string[] = [];
            for (let block in user.content) {
                match (block) {
                    let text: ai.content.Text => parts.push(text.text),
                    let media: ai.content.Media => {
                        throw _invalid(`Jev does not support ${media.kind()} input`)
                    },
                };
            }
            let content = parts.join("").trim();
            if (content != "") {
                messages.push({ "role": "user", "content": content });
            }
        } else if let assistant: ai.events.AssistantMessage = event {
            let parts: string[] = [];
            for (let block in assistant.content) {
                match (block) {
                    let text: ai.content.Text => parts.push(text.text),
                    let media: ai.content.Media => {
                        throw _invalid(`Jev does not support ${media.kind()} input`)
                    },
                    let reasoning: ai.content.Reasoning => null,
                    let use: ai.content.ToolUse => throw _invalid("Jev does not support tool calls"),
                };
            }
            let content = parts.join("").trim();
            if (content != "") {
                messages.push({ "role": "assistant", "content": content });
            }
        } else if let completed: ai.events.ToolCompleted = event {
            throw _invalid("Jev does not support tool results in conversation history");
        } else if let failed: ai.events.ToolFailed = event {
            throw _invalid("Jev does not support tool results in conversation history");
        }
    }
    if (messages.length() == 0) {
        throw _invalid("Jev requires a nonempty text state");
    }
    if (messages.length() == 1) {
        let only: map<string, baml.json.json> = messages.at(0) else {
            throw _invalid("Jev requires a nonempty text state");
        };
        if (only.get("role") == "user") {
            return only.get("content") ?? "";
        }
    }
    { "messages": messages }
}

function _request(
    c: root.Client,
    state: baml.json.json,
    questions: map<string, baml.json.json>,
    preview: bool,
) -> baml.http.Request {
    let body: map<string, baml.json.json> = { "state": state, "model": c.model, "questions": questions };
    let headers: map<string, string> = { "content-type": "application/json" };
    if (!preview) {
        let _ = headers.set("authorization", `Bearer ${c.resolved_api_key()}`);
    }
    baml.http.Request {
        method: "POST",
        url: `${_origin(c)}/v1/systemone`,
        headers: headers,
        body: baml.json.stringify(body),
    }
}

function _origin(c: root.Client) -> string {
    let resolved = ai.wire.resolve_credential(c.base_url, null);
    if (c.base_url != null && resolved == null) {
        throw _invalid(`base_url did not resolve: ${ai.wire.credential_sources(c.base_url, null)}`);
    }
    let base = (resolved ?? "https://api.typesafe.ai").trim();
    if (base == "") {
        throw _invalid("base_url must not be empty");
    }
    if (!base.starts_with("https://") || base == "https://") {
        throw _invalid("base_url must be an HTTPS origin");
    }
    if (base.ends_with("/")) {
        base.slice(0, -1)
    } else {
        base
    }
}

function list_models_request(c: root.Client, preview: bool = false) -> baml.http.Request {
    let headers: map<string, string> = {};
    if (!preview) {
        let _ = headers.set("authorization", `Bearer ${c.resolved_api_key()}`);
    }
    baml.http.Request { method: "GET", url: `${_origin(c)}/v1/models`, headers: headers, body: "" }
}

function list_models(c: root.Client) -> root.ModelsResponse {
    ai.wire.send_as<root.ModelsResponse>(
        list_models_request(c),
        "typesafeai",
        request_timeout_ms = c.request_timeout_ms,
    )
}

function system_one_request(
    c: root.Client,
    state: baml.json.json,
    defined_questions: map<string, root.Question>,
    preview: bool = false,
) -> baml.http.Request {
    if (defined_questions.keys().length() == 0) {
        throw _invalid("System One requires at least one question");
    }
    let questions: map<string, baml.json.json> = {};
    for (let key in defined_questions.keys()) {
        let question: root.Question = defined_questions.get(key) else {
            throw _invalid(`missing question ${key}`);
        };
        if (question.type != "noul" && question.criteria == null) {
            throw _invalid(`${question.type} question ${key} requires criteria`);
        }
        if (question.type == "choice") {
            let choices = baml.json.to<map<string, baml.json.json>>(question.criteria) catch_all (e) {
                _ => throw _invalid(`choice question ${key} requires an object of choices`),
            };
            if (choices.keys().length() < 2) {
                throw _invalid(`choice question ${key} requires at least two choices`);
            }
        }
        if (question.type == "score") {
            let levels = baml.json.to<string[]>(question.criteria) catch_all (e) {
                _ => throw _invalid(`score question ${key} requires an array of levels`),
            };
            if (levels.length() < 2) {
                throw _invalid(`score question ${key} requires at least two levels`);
            }
        }
        let wire_question: map<string, baml.json.json> = { "type": question.type, "instructions": question.instructions };
        if (question.criteria != null) {
            let _ = wire_question.set("criteria", question.criteria);
        }
        let _ = questions.set(key, wire_question);
    }
    _request(c, state, questions, preview)
}

function system_one(
    c: root.Client,
    state: baml.json.json,
    questions: map<string, root.Question>,
) -> root.Response {
    let req = system_one_request(c, state, questions);
    ai.wire.send_as<root.Response>(req, "typesafeai", request_timeout_ms = c.request_timeout_ms)
}

function render(
    c: root.Client,
    input: ai.ModelTurnInput,
    preview: bool = false,
) -> baml.http.Request {
    let state = _state(input, c.id());
    let instructions = c.instructions ?? "Answer the question posed in the state.";
    system_one_request(c, state, _questions(c, input.output_type, instructions), preview = preview)
}

function _answer_value(
    response: root.Response,
    key: string,
    output: reflect.Type,
    threshold: float,
) -> baml.json.json {
    let answer: root.Answer = response.answers.get(key) else {
        throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: `response has no ${key} answer` };
    };
    if (output == reflect.Type.of<bool>() && answer.type == "noul") {
        if let probability: float = answer.noul {
            if (probability < 0.0 || probability > 1.0) {
                throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: "noul probability is outside 0..1" };
            }
            return probability >= threshold;
        }
    }
    if ((output.as_enum() != null || output.as_union() != null) && answer.type == "choice") {
        if let choice: string = answer.choice {
            return choice;
        }
    }
    if (output == reflect.Type.of<float>()) {
        if (answer.type == "noul") {
            if let probability: float = answer.noul {
                if (probability >= 0.0 && probability <= 1.0) {
                    return probability;
                }
            }
        }
        if (answer.type == "score") {
            if let score: float = answer.score {
                return score;
            }
        }
    }
    throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: `response answer ${key} did not match ${output.to_string()}` }
}

function _value(
    response: root.Response,
    output: reflect.Type,
    path: string,
    threshold: float,
    depth: int,
) -> baml.json.json {
    if (depth >= 16) {
        throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: "nested class depth exceeds 16" };
    }
    if let class_type: reflect.class.Type = output.as_class() {
        let fields: map<string, baml.json.json> = {};
        for (let field in class_type.fields()) {
            let field_type: reflect.Type = field.type else {
                throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: `class field ${field.name} has no reflected type` };
            };
            let name = field.meta.alias ?? field.name;
            if (fields.keys().includes(name)) {
                throw ai.errors.ParseFailed { provider: "typesafeai", raw_output: `class has duplicate output key ${name}` };
            }
            let key = if (path == "") {
                field.name
            } else {
                `${path}.${field.name}`
            };
            let _ = fields.set(name, _value(response, field_type, key, threshold, depth + 1));
        }
        return fields;
    }
    _answer_value(
        response,
        if (path == "") {
            "result"
        } else {
            path
        },
        output,
        threshold,
    )
}

function _text(response: root.Response, output: reflect.Type, threshold: float) -> string {
    baml.json.stringify(_value(response, output, "", threshold, 0))
}

function invoke(c: root.Client, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = render(c, input);
    let sent = ai.wire.send_as_with_wire<root.Response>(
        req,
        "typesafeai",
        c.id(),
        capture_wire = c.capture_wire,
        request_timeout_ms = c.request_timeout_ms,
    );
    let text = _text(sent.value, input.output_type, c.noul_threshold);
    let usage = ai.events.Usage {
        input_tokens: sent.value.usage.input_tokens,
        output_tokens: sent.value.usage.output_tokens,
        cached_input_tokens: null,
        reasoning_tokens: null,
    };
    ai.ModelTurn {
        content: [ai.content.Text { text: text }],
        stop_reason: ai.content.StopReason.Complete,
        usage: usage,
        calls: [sent.call],
    }
}
