// OpenAI-specific schema and response-envelope handling. The core
// baml.schema.json_schema function intentionally remains provider-neutral.

function _output_tool_name() -> string throws never {
    "__baml_return_output"
}

function _is_strict(provider: root.openai.OpenAIProvider) -> bool throws never {
    provider.output_mode == root.openai.OutputMode.Strict
}

function _uses_sap(provider: root.openai.OpenAIProvider) -> bool throws never {
    provider.output_mode == root.openai.OutputMode.Sap
}

function _validate_output_config(
    provider: root.openai.OpenAIProvider,
) -> null throws root.ai.InvalidRequest {
    if (
        provider.tool_mode == root.openai.ToolMode.Prompt
            && provider.output_mode != root.openai.OutputMode.Sap
    ) {
        throw root.ai.InvalidRequest {
            provider: "openai-responses",
            status_code: null,
            detail: "`tool_mode = Prompt` requires `output_mode = Sap`",
        };
    }
    null
}

// Lower provider-neutral JSON Schema to the subset accepted by OpenAI strict
// function tools: every object is closed and every property is required.
// Optional BAML fields remain nullable in their property schema.
function _strict_schema(schema: json, path: string) -> json throws baml.errors.Unsupported {
    match (schema) {
        let object: map<string, json> => {
            if (object.keys().length() == 0) {
                throw baml.errors.Unsupported {
                    message: "OpenAI strict mode cannot represent unconstrained JSON at " + path,
                };
            }
            for (let key in object.keys()) {
                // These keywords contain maps *of schemas*. The container may
                // legitimately be empty, as it is for an empty BAML class.
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
                                        _strict_schema(schemas[name], path + "." + key + "." + name),
                                    );
                            }
                            object.set(key, schemas);
                        },
                        let value: json => {
                            object.set(key, _strict_schema(value, path + "." + key));
                        },
                    }
                } else {
                    object.set(key, _strict_schema(object[key], path + "." + key));
                }
            }
            match (object.get("type")) {
                "object" => {
                    match (object.get("properties")) {
                        let properties: map<string, json> => {
                            object
                                .set(
                                    "required",
                                    baml.json.from(properties.keys()) catch (e) {
                                        _ => {
                                            throw baml.errors.Unsupported {
                                            message: "could not encode strict property names at "
                                                + path,
                                        }
                                        },
                                    },
                                );
                            object.set("additionalProperties", false);
                        },
                        _ => {
                            throw baml.errors.Unsupported {
                            message: "OpenAI strict mode cannot represent a map-like object at "
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
                _strict_schema(item, path + "[]")
            })
        },
        _ => schema,
    }
}

function _strict_tool_input_schema(schema: json) -> json throws baml.errors.Unsupported {
    let cloned = baml.json.parse(baml.json.stringify(schema)) catch (e) {
        _ => {
            throw baml.errors.Unsupported {
            message: "OpenAI tool input schema was not valid JSON",
        }
        },
    };
    let strict = _strict_schema(cloned, "$tool");
    match (strict) {
        let object: map<string, json> => {
            match (object.get("type")) {
                "object" if object.has("properties") => object,
                _ => {
                    throw baml.errors.Unsupported {
                    message: "OpenAI function tools require an object input schema",
                }
                },
            }
        },
        _ => {
            throw baml.errors.Unsupported {
            message: "OpenAI function tools require an object input schema",
        }
        },
    }
}

// Responses function tools are flat: name, description, parameters, and
// strict are siblings of type. The wrapper keeps every possible T legal at
// the object-only function-parameter root.
function _output_tool(
    output_type: type,
    strict: bool,
) -> map<string, json> throws baml.errors.Unsupported {
    let value_schema = baml.schema.json_schema(output_type);
    if (strict) {
        value_schema = _strict_schema(value_schema, "$output.value");
    }
    let schema_object = match (value_schema) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.Unsupported {
            message: "OpenAI output schema must be a JSON Schema object",
        }
        },
    };
    // $ref values remain rooted at the whole parameters document.
    let has_definitions = schema_object.has("$defs");
    let definitions = schema_object.get("$defs");
    if (has_definitions) {
        schema_object.delete("$defs");
    }
    let parameters: map<string, json> = {
        "type": "object",
        "properties": { "value": schema_object },
        "required": ["value"],
        "additionalProperties": false,
    };
    if (has_definitions) {
        parameters.set("$defs", definitions);
    }
    {
        "type": "function",
        "name": _output_tool_name(),
        "description": "Return the final typed result. Call this only when the task is complete.",
        "parameters": parameters,
        "strict": strict,
    }
}

function _application_tool(
    tool: root.ai.tools.Tool,
    strict: bool,
) -> json throws baml.errors.Unsupported {
    {
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": if (strict) {
            _strict_tool_input_schema(tool.input_schema)
        } else {
            tool.input_schema
        },
        "strict": strict,
    }
}

function _application_tools(
    tools: root.ai.tools.Tool[],
    strict: bool,
) -> json[] throws baml.errors.Unsupported {
    tools.map((tool: root.ai.tools.Tool) -> json {
        _application_tool(tool, strict)
    })
}

function _agent_tools(
    output_type: type,
    tools: root.ai.tools.Tool[],
    strict: bool,
    use_sap: bool,
) -> json[] throws baml.errors.Unsupported {
    // OpenAI's direct SAP agent mode is intentionally hybrid:
    // - application tools remain Responses function tools;
    // - the final T is output_text parsed by SAP.
    // OpenAI's prompt tool mode delegates to the provider-neutral prompt
    // adapter before this helper; it encodes T | ToolCalls in the prompt.
    if (
        !use_sap
            && tools.some((tool: root.ai.tools.Tool) -> bool {
                tool.name == _output_tool_name()
            })
    ) {
        throw baml.errors.Unsupported {
            message: "OpenAI reserves the tool name " + _output_tool_name(),
        };
    }
    let encoded = _application_tools(tools, strict);
    if (!use_sap) {
        encoded.push(_output_tool(output_type, strict));
    }
    encoded
}

function _output_call_value(call: json, raw_output: string) -> json throws root.ai.ParseFailed {
    if (baml.json.path_or(call, ".name", "") != _output_tool_name()) {
        throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        };
    }
    let arguments = baml.json.path<string>(call, ".arguments") catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        }
        },
    };
    let wrapper = baml.json.parse(arguments) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        }
        },
    };
    baml.json.path<json>(wrapper, ".value") catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        }
        },
    }
}

function _parse_output_call<T>(call: json, raw_output: string) -> T throws root.ai.ParseFailed {
    baml.json.from_json<T>(_output_call_value(call, raw_output)) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        }
        },
    }
}

function _response_function_calls(raw: json) -> json[] throws never {
    let calls: json[] = [];
    for (let item in baml.json.path_or<json[]>(raw, ".output", [])) {
        if (baml.json.path_or<string>(item, ".type", "") == "function_call") {
            calls.push(item);
        }
    }
    calls
}

function _response_output_text(raw: json) -> string? throws never {
    for (let item in baml.json.path_or<json[]>(raw, ".output", [])) {
        for (let content in baml.json.path_or<json[]>(item, ".content", [])) {
            if (baml.json.path_or<string>(content, ".type", "") == "output_text") {
                return baml.json.path_or<string?>(content, ".text", null);
            }
        }
    }
    null
}

function _response_refusal(raw: json) -> string? throws never {
    for (let item in baml.json.path_or<json[]>(raw, ".output", [])) {
        for (let content in baml.json.path_or<json[]>(item, ".content", [])) {
            if (baml.json.path_or<string>(content, ".type", "") == "refusal") {
                return baml.json.path_or<string?>(content, ".refusal", null);
            }
        }
    }
    null
}

function _response_metadata(
    raw: json,
    fallback_model: string,
) -> root.ai.ResponseMetadata throws never {
    let usage_json = baml.json.path_or<json>(raw, ".usage", null);
    root.ai.ResponseMetadata {
        provider: "openai",
        model: baml.json.path_or<string?>(raw, ".model", fallback_model),
        request_id: baml.json.path_or<string?>(raw, ".id", null),
        finish_reason: baml.json.path_or<string?>(raw, ".status", null),
        usage: match (usage_json) {
            let usage: map<string, json> => {
                root.ai.Usage {
                    input_tokens: baml.json.path_or(usage, ".input_tokens", 0),
                    output_tokens: baml.json.path_or(usage, ".output_tokens", 0),
                    cached_input_tokens: baml.json.path_or<int?>(usage, ".input_tokens_details.cached_tokens", null),
                    reasoning_tokens: baml.json.path_or<int?>(usage, ".output_tokens_details.reasoning_tokens", null),
                    cost_usd: null,
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
) -> T throws root.ai.Failure {
    match (_response_refusal(raw)) {
        let refusal: string => {
            throw root.ai.Refused {
            provider: "openai-responses",
            reason: refusal,
        }
        },
        null => {},
    }
    let status = baml.json.path_or(raw, ".status", "");
    if (status == "failed" || status == "cancelled" || status == "incomplete") {
        throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        };
    }
    if (use_sap) {
        // ctx.output_format is the model-visible schema in SAP mode. SAP can
        // recover any T it supports, including a union. No union branch is
        // reinterpreted as a provider function tool.
        let text = match (_response_output_text(raw)) {
            let value: string => value,
            null => {
                throw root.ai.ParseFailed {
                provider: "openai-responses",
                raw_output: raw_output,
            }
            },
        };
        return baml.sap.parse<T>(text) catch (e) {
            _ => {
                throw root.ai.ParseFailed {
                provider: "openai-responses",
                raw_output: text,
            }
            },
        };
    }
    let calls = _response_function_calls(raw);
    if (calls.length() != 1) {
        throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: raw_output,
        };
    }
    _parse_output_call<T>(calls[0], raw_output)
}
