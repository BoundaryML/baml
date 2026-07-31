function _uses_sap(provider: root.anthropic.AnthropicProvider) -> bool throws never {
    provider.output_mode == root.anthropic.OutputMode.Sap
}

function _validate_output_config(
    provider: root.anthropic.AnthropicProvider,
) -> null throws ai.InvalidRequest {
    if (
        provider.tool_mode == root.anthropic.ToolMode.Prompt
            && provider.output_mode != root.anthropic.OutputMode.Sap
    ) {
        throw ai.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "`tool_mode = Prompt` requires `output_mode = Sap`",
        };
    }
    null
}

// Anthropic strict structured outputs require closed object schemas. Keep the
// original required list: unlike OpenAI strict function tools, optional fields
// do not become required merely because strict mode is enabled.
function _closed_schema(schema: json, path: string) -> json throws baml.errors.Unsupported {
    match (schema) {
        let object: map<string, json> => {
            if (object.keys().length() == 0) {
                throw baml.errors.Unsupported {
                    message: "Anthropic strict mode cannot represent unconstrained JSON at " + path,
                };
            }
            for (let key in object.keys()) {
                // These keywords contain maps *of schemas*. The container map
                // may legitimately be empty (for example an empty class has
                // `properties: {}`); only each value is itself a schema.
                if (
                    key == "properties"
                        || key == "$defs"
                        || key == "definitions"
                        || key == "patternProperties"
                        || key == "dependentSchemas"
                ) {
                    match (object[key]) {
                        let schemas: map<string, json> => {
                            for (let name in schemas.keys()) {
                                schemas
                                    .set(
                                        name,
                                        _closed_schema(schemas[name], path + "." + key + "." + name),
                                    );
                            }
                            object.set(key, schemas);
                        },
                        let value: json => {
                            object.set(key, _closed_schema(value, path + "." + key));
                        },
                    }
                } else {
                    object.set(key, _closed_schema(object[key], path + "." + key));
                }
            }
            match (object.get("type")) {
                "object" => {
                    match (object.get("properties")) {
                        let properties: map<string, json> => {
                            object.set("additionalProperties", false);
                        },
                        _ => {
                            throw baml.errors.Unsupported {
                                message: "Anthropic strict mode cannot represent a map-like object at "
                                    + path,
                            }
                        },
                    }
                },
                _ => {},
            }
            object
        },
        let array: json[] => {
            array.map((item: json) -> json {
                _closed_schema(item, path + "[]")
            })
        },
        _ => schema,
    }
}

function _output_schema(output_type: type) -> json throws baml.errors.Unsupported {
    let schema = baml.schema.json_schema(output_type);
    let cloned = baml.json.parse(baml.json.stringify(schema)) catch (e) {
        _ => {
            throw baml.errors.Unsupported {
                message: "Anthropic output schema was not valid JSON",
            }
        },
    };
    _closed_schema(cloned, "$output")
}

function _tool_input_schema(schema: json, strict: bool) -> json throws baml.errors.Unsupported {
    if (!strict) {
        return schema;
    }
    let cloned = baml.json.parse(baml.json.stringify(schema)) catch (e) {
        _ => {
            throw baml.errors.Unsupported {
                message: "Anthropic tool input schema was not valid JSON",
            }
        },
    };
    let closed = _closed_schema(cloned, "$tool");
    match (closed) {
        let object: map<string, json> => {
            match (object.get("type")) {
                "object" if object.has("properties") => object,
                _ => {
                    throw baml.errors.Unsupported {
                        message: "Anthropic tools require an object input schema",
                    }
                },
            }
        },
        _ => {
            throw baml.errors.Unsupported {
                message: "Anthropic tools require an object input schema",
            }
        },
    }
}

function _response_content(raw: json) -> json[] throws never {
    baml.json.path_or<json[]>(raw, ".content", [])
}

function _response_text(raw: json) -> string? throws never {
    let parts: string[] = [];
    let response_content = _response_content(raw);
    for (let content in response_content) {
        let content_type = baml.json.path_or(content, ".type", "");
        if (content_type == "text") {
            match (baml.json.path_or<string?>(content, ".text", null)) {
                let text: string => {
                    parts.push(text);
                },
                null => {},
            }
        }
    }
    if (parts.length() == 0) {
        return null;
    }
    parts.join("")
}

// Visible extended-thinking text for the ModelStep display channel. The
// signed thinking blocks themselves stay in the conversation's exact
// assistant content; this is only the human-readable projection.
function _response_thinking_text(raw: json) -> string? throws never {
    let parts: string[] = [];
    let response_content = _response_content(raw);
    for (let content in response_content) {
        let content_type = baml.json.path_or(content, ".type", "");
        if (content_type == "thinking") {
            match (baml.json.path_or<string?>(content, ".thinking", null)) {
                let text: string => {
                    parts.push(text);
                },
                null => {},
            }
        }
    }
    if (parts.length() == 0) {
        return null;
    }
    parts.join("\n")
}

function _response_tool_uses(raw: json) -> json[] throws never {
    let content = _response_content(raw);
    let calls: json[] = [];
    for (let part in content) {
        if (_is_tool_use(part)) {
            calls.push(part);
        }
    }
    calls
}

function _response_metadata(
    raw: json,
    fallback_model: string,
) -> ai.ResponseMetadata throws never {
    let usage_json = baml.json.path_or<json>(raw, ".usage", null);
    ai.ResponseMetadata {
        provider: "anthropic",
        model: baml.json.path_or<string?>(raw, ".model", fallback_model),
        request_id: baml.json.path_or<string?>(raw, ".id", null),
        finish_reason: baml.json.path_or<string?>(raw, ".stop_reason", null),
        usage: match (usage_json) {
            let usage: map<string, json> => {
                ai.Usage {
                    input_tokens: baml.json.path_or(usage, ".input_tokens", 0),
                    output_tokens: baml.json.path_or(usage, ".output_tokens", 0),
                    cached_input_tokens: baml.json.path_or<int?>(usage, ".cache_read_input_tokens", null),
                    reasoning_tokens: null,
                }
            },
            _ => null,
        },
        attributes: {},
        raw: raw,
    }
}

function _parse_response_value<T>(
    raw: json,
    raw_output: string,
    use_sap: bool,
) -> T throws ai.Failure {
    let stop_reason = baml.json.path_or(raw, ".stop_reason", "");
    if (stop_reason == "refusal") {
        throw ai.Refused {
            provider: "anthropic",
            reason: _response_text(raw) ?? "Anthropic refused the request",
        };
    }
    if (
        stop_reason == "max_tokens"
            || stop_reason == "model_context_window_exceeded"
            || stop_reason == "tool_use"
    ) {
        throw ai.ParseFailed {
            provider: "anthropic",
            raw_output: raw_output,
        };
    }
    let text = match (_response_text(raw)) {
        let value: string => value,
        null => {
            throw ai.ParseFailed {
                provider: "anthropic",
                raw_output: raw_output,
            }
        },
    };
    if (use_sap) {
        return baml.sap.parse<T>(text) catch (e) {
            _ => {
                throw ai.ParseFailed {
                    provider: "anthropic",
                    raw_output: text,
                }
            },
        };
    }
    let value = baml.json.parse(text) catch (e) {
        _ => {
            throw ai.ParseFailed {
                provider: "anthropic",
                raw_output: text,
            }
        },
    };
    baml.json.from_json<T>(value) catch (e) {
        _ => {
            throw ai.ParseFailed {
                provider: "anthropic",
                raw_output: text,
            }
        },
    }
}
