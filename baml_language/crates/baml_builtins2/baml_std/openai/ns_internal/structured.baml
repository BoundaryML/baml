// OpenAI Structured Outputs, shared by the Chat Completions and Responses
// clients: the strict JSON Schema a request carries (`response_format` on chat,
// `text.format` on responses) and the strict reading of the answer it holds
// the model to.
//
// The schema is `baml.json.schema` under aliased names, rewritten to the
// subset strict mode accepts. The answer is read by `baml.json.from_string`
// under the same names, so what the schema allows is what the decoder takes.
// A type strict mode cannot hold is rejected before anything is sent.

/// (internal) Structured output for one output type: the schema a request
/// carries, and how the answer is read back.
class StructuredOutput {
    output_type: reflect.Type,
    /// The strict JSON Schema.
    schema: baml.json.json,
    /// Strict mode wants an object at the root. Any other output type is sent
    /// as the one property `value` of an object, and read back out of it.
    wrapped: bool,

    /// The `name` the schema goes by on the wire.
    function name(self) -> string {
        "output"
    }

    /// The answer `text` as a value of the output type, read strictly: the
    /// JSON must have the shape the schema describes, under the names it uses.
    /// Anything else is `ai.errors.ParseFailed`, never a lenient parse.
    function decode(self, provider: string, text: string) -> ai.ParsedOutput {
        let failed = (message: string) -> ai.errors.ParseFailed {
            ai.errors.ParseFailed {
                provider: provider,
                raw_output: text,
                message: `the structured output does not fit \`${self.output_type.to_string()}\`: ${message}`,
            }
        };
        let body = if (self.wrapped) {
            let parsed = baml.json.parse(text) catch (e) {
                let err: baml.json.ParseError => {
                    throw failed(err.message)
                },
            };
            let object: map<string, baml.json.json> = parsed else {
                throw failed("expected a JSON object holding `value`")
            };
            if (!object.has("value")) {
                throw failed("the JSON object has no `value`")
            }
            baml.json.stringify(object.get("value"))
        } else {
            text
        };
        let type T = self.output_type;
        let value = baml.json.from_string<T>(body, naming = baml.json.Naming.Aliased) catch (e) {
            let err: baml.json.ParseError => {
                throw failed(err.message)
            },
            let err: baml.json.DecodeError => {
                throw failed(if (err.path == "") {
                    err.message
                } else {
                    `${err.message} at ${err.path}`
                })
            },
        };
        ai.ParsedOutput { value: value }
    }
}

/// (internal) The structured output for `output_type`. A type strict mode cannot hold
/// is rejected here, before a request is sent, as `ai.errors.InvalidRequest`:
/// one with no JSON form (`baml.json.schema` says why), one that can hold
/// media, and one with a map in it.
function structured_output(output_type: reflect.Type, provider: string) -> StructuredOutput {
    let type_name = output_type.to_string();
    let reaches_media = ai.internal.reaches_media(output_type) catch (e) {
        let err: baml.errors.Unsupported => {
            throw _reject(provider, `structured outputs cannot describe \`${type_name}\`: ${err.message}`)
        },
    };
    if (reaches_media) {
        throw _reject(
            provider,
            `structured outputs cannot hold \`${type_name}\`: it can hold media, and a model returns the media it generates as content of its own, not inside JSON`,
        )
    }
    let lowered = baml.json.schema(output_type, naming = baml.json.Naming.Aliased) catch (e) {
        let err: baml.errors.Unsupported => {
            throw _reject(provider, `structured outputs cannot describe \`${type_name}\`: ${err.message}`)
        },
    };
    let top: map<string, baml.json.json> = lowered else {
        throw _reject(provider, `structured outputs cannot describe \`${type_name}\`: its schema is not an object`)
    };
    let wrapped = !_is_object_schema(top);
    let schema: baml.json.json = if (wrapped) {
        let inner: map<string, baml.json.json> = {};
        for (let key in top.keys()) {
            if (key != "$defs") {
                let _ = inner.set(key, top.get(key));
            }
        }
        let properties: map<string, baml.json.json> = { "value": inner };
        let wrapper: map<string, baml.json.json> = {
            "type": "object",
            "properties": properties,
            "required": ["value"],
            "additionalProperties": false,
        };
        if (top.has("$defs")) {
            let _ = wrapper.set("$defs", top.get("$defs"));
        }
        wrapper
    } else {
        top
    };
    StructuredOutput {
        output_type: output_type,
        schema: _strict(schema, type_name, provider),
        wrapped: wrapped,
    }
}

/// (internal) Whether `schema` is a plain object schema, which strict mode takes at
/// the root as it is: not a union (`anyOf`), a reference, or a nullable object.
function _is_object_schema(schema: map<string, baml.json.json>) -> bool {
    if (schema.has("anyOf") || schema.has("$ref")) {
        return false;
    }
    match (schema.get("type")) {
        let kind: string => kind == "object",
        _ => false,
    }
}

/// (internal) `schema`, from `baml.json.schema`, rewritten to the subset OpenAI's strict
/// mode accepts:
///
///   * every object lists all its properties in `required` and sets
///     `additionalProperties: false`. A property that may be absent is one
///     whose schema already admits `null`, and strict mode spells "optional"
///     exactly that way;
///   * a `$ref` with a `description` beside it becomes a one-member `anyOf`,
///     because strict mode rejects keywords beside `$ref`;
///   * a map (an object with schema-valued `additionalProperties`, or
///     `propertyNames`) is rejected: strict mode has no form for one.
///
/// Only the keywords that hold schemas are walked. `enum` and `const` hold
/// values, and are copied as they are.
function _strict(schema: baml.json.json, type_name: string, provider: string) -> baml.json.json {
    let object: map<string, baml.json.json> = schema else {
        return schema;
    };
    let out: map<string, baml.json.json> = {};
    for (let key in object.keys()) {
        let value = object.get(key);
        let rewritten: baml.json.json = match (key) {
            "properties" | "$defs" => {
                let members: map<string, baml.json.json> = value else {
                    throw _reject(provider, `structured outputs cannot describe \`${type_name}\`: \`${key}\` is not an object`)
                };
                let walked: map<string, baml.json.json> = {};
                for (let name in members.keys()) {
                    let _ = walked.set(name, _strict(members.get(name), type_name, provider));
                }
                walked
            },
            "anyOf" => {
                let members: baml.json.json[] = value else {
                    throw _reject(provider, `structured outputs cannot describe \`${type_name}\`: \`anyOf\` is not a list`)
                };
                let walked: baml.json.json[] = members.map((member) -> {
                    _strict(member, type_name, provider)
                });
                walked
            },
            "items" => _strict(value, type_name, provider),
            "additionalProperties" => {
                if (value != false) {
                    throw _reject(
                        provider,
                        `structured outputs cannot describe \`${type_name}\`: it holds a map, which strict mode has no form for`,
                    )
                }
                value
            },
            "propertyNames" => {
                throw _reject(
                    provider,
                    `structured outputs cannot describe \`${type_name}\`: it holds a map, which strict mode has no form for`,
                )
            },
            _ => value,
        };
        let _ = out.set(key, rewritten);
    }

    if (_is_object_type(out.get("type"))) {
        let properties: map<string, baml.json.json> = match (out.get("properties")) {
            let present: map<string, baml.json.json> => present,
            _ => {
                let none: map<string, baml.json.json> = {};
                none
            },
        };
        let required: baml.json.json[] = [];
        for (let name in properties.keys()) {
            required.push(name);
        }
        let _ = out.set("properties", properties);
        let _ = out.set("required", required);
        let _ = out.set("additionalProperties", false);
    }

    if (out.has("$ref") && out.length() > 1) {
        let reference: map<string, baml.json.json> = { "$ref": out.get("$ref") };
        let wrapped: map<string, baml.json.json> = { "anyOf": [reference] };
        for (let key in out.keys()) {
            if (key != "$ref") {
                let _ = wrapped.set(key, out.get(key));
            }
        }
        return wrapped;
    }
    out
}

/// (internal) Whether a schema's `type` is, or includes, `object`.
function _is_object_type(kind: baml.json.json) -> bool {
    match (kind) {
        let one: string => one == "object",
        let many: baml.json.json[] => {
            many.some((member) -> {
                member == "object"
            })
        },
        _ => false,
    }
}

/// (internal) `turn` with its answer read through `structured`. A turn that calls
/// tools, was cut short, or refused holds no answer to read, and goes to the
/// runner as it is; any other turn's last text must decode, or this throws.
/// So `parsed_output` is never left for the runner to fill with SAP.
function with_structured_output(
    turn: ai.ModelTurn,
    structured: StructuredOutput,
    provider: string,
) -> ai.ModelTurn {
    if (
        turn.tool_uses().length() > 0
            || turn.stop_reason == ai.content.StopReason.MaxTokens
            || turn.stop_reason == ai.content.StopReason.Refused
    ) {
        return turn;
    }
    let text = turn.terminal_text() ?? "";
    ai.ModelTurn { ...turn, parsed_output: structured.decode(provider, text) }
}
