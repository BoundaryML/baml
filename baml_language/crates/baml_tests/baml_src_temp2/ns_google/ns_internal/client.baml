class HttpResult {
    raw: json,
    text: string,
}

function _body_object(
    request: baml.http.Request,
    provider_name: string,
) -> map<string, json> throws baml.errors.UnknownError {
    let parsed = baml.json.parse(request.body) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " request body was not JSON",
        )
        },
    };
    match (parsed) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            parsed,
            provider_name + " request body was not an object",
        )
        },
    }
}

function _clone_body(
    body: json,
    provider_name: string,
) -> map<string, json> throws baml.errors.UnknownError {
    let cloned = baml.json.parse(baml.json.stringify(body)) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " could not clone its request body",
        )
        },
    };
    match (cloned) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            cloned,
            provider_name + " request body template was not an object",
        )
        },
    }
}

function _built_request<T>(
    primitive: baml.llm.PrimitiveClient,
    task: root.ai.Task<T>,
    provider_name: string,
) -> baml.http.Request throws baml.errors.UnknownError {
    let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " prompt specialization failed",
        )
        },
    };
    primitive.build_request(specialized, task.output_type()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " request build failed",
        )
        },
    }
}

function _send_json(
    request: baml.http.Request,
    provider_name: string,
) -> HttpResult throws root.ai.Failure | baml.errors.UnknownError {
    let response = baml.http.send(request) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " send failed",
        )
        },
    };
    let text = response.text() catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " response body read failed",
        )
        },
    };
    if (!response.ok()) {
        throw root.ai.internal.classify_http(provider_name, response.status_code, text);
    }
    let raw = baml.json.parse(text) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: text,
        }
        },
    };
    HttpResult { raw: raw, text: text }
}

function _generation_body(
    body: map<string, json>,
    output_type: type,
    use_sap: bool,
) -> map<string, json> throws baml.errors.Unsupported {
    // Generation is a typed value request, never an application-tool turn.
    body.delete("tools");
    body.delete("toolConfig");
    if (!use_sap) {
        let config = match (body.get("generationConfig")) {
            let existing: map<string, json> => existing,
            _ => {
                let created: map<string, json> = {};
                created
            },
        };
        config.set("responseMimeType", "application/json");
        config.set("responseJsonSchema", _output_value_schema(output_type));
        body.set("generationConfig", config);
    } else {
        // SAP relies on ctx.output_format and text parsing. Provider-owned
        // fields win if an extra_body accidentally supplied a native schema.
        match (body.get("generationConfig")) {
            let config: map<string, json> => {
                config.delete("responseMimeType");
                config.delete("responseJsonSchema");
                config.delete("responseSchema");
                body.set("generationConfig", config);
            },
            _ => {},
        }
    }
    body
}

function _generate<T>(
    primitive: baml.llm.PrimitiveClient,
    task: root.ai.Task<T>,
    provider_name: string,
    model: string,
    use_sap: bool,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    let request = _generation_request(primitive, task, provider_name, use_sap);
    log.info(
        baml.json
            .stringify(
                {
                    "event": "llm.request",
                    "provider": provider_name,
                    "model": model,
                    "request_bytes": request.body.length(),
                    "output_mode": if (use_sap) {
                        "sap"
                    } else {
                        "native"
                    },
                },
            ),
    );
    let result = _send_json(request, provider_name);
    _validate_response(result.raw, result.text, provider_name);
    root.ai.ResponseWithMetadata<T> {
        value: _parse_generation_value<T>(result.raw, result.text, provider_name, use_sap),
        metadata: _response_metadata(result.raw, model, provider_name),
        conversation: null,
    }
}

// Kept as a separate internal operation so request-shape tests can inspect the
// exact authenticated REST request without sending it.
function _generation_request<T>(
    primitive: baml.llm.PrimitiveClient,
    task: root.ai.Task<T>,
    provider_name: string,
    use_sap: bool,
) -> baml.http.Request throws root.ai.Failure | baml.errors.UnknownError {
    let built = _built_request(primitive, task, provider_name);
    let body = _generation_body(_body_object(built, provider_name), task.output_type(), use_sap);
    baml.http.Request {
        method: built.method,
        url: built.url,
        headers: built.headers,
        body: baml.json.stringify(body),
    }
}

function _complete<T>(
    generation: root.ai.GenerationProvider,
    tool_provider: root.ai.tools.ToolCallingProvider,
    task: root.ai.Task<T>,
    provider_name: string,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Use one typed generation when no application tools are active
    if (task.tools.length() == 0) {
        return generation.generate<T>(task.with_provider(generation));
    }
    let max_steps = 32;
    let tools = task.tools;
    //# Begin the provider-owned Gemini function-call conversation
    let conversation = tool_provider.begin<T>(task.with_provider(generation));
    let usage = root.ai.Usage {
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        cost_usd: 0.0,
    };
    let steps = 0;
    //# Continue until Gemini returns the reserved typed output
    while (steps < max_steps) {
        //## Ask for a final T or application function calls
        let result = tool_provider.step<T>(conversation, tools);
        steps = steps + 1;
        match (result.metadata.usage) {
            let step_usage: root.ai.Usage => root.ai.internal._add_usage(usage, step_usage),
            null => {},
        }
        match (result.outcome) {
            let calls: root.ai.tools.ToolCalls => {
                // Completion<T> cannot surface a handoff outcome. Callers that
                // need handoffs should select the Agent runner explicitly.
                for (let call in calls.calls) {
                    if (
                        tools
                            .find((tool: root.ai.tools.Tool) -> bool {
                                tool.name == call.name
                            })
                            ?.handoff
                            ?? false
                    ) {
                        throw baml.errors.UnknownError {
                            data: {
                                "provider": provider_name,
                                "tool": call.name,
                                "steps_taken": steps,
                            },
                            message: [
                                provider_name
                                    + " completion reached a handoff; use ai.run.Agent to receive it",
                            ],
                        }
                    }
                }
                //## Execute application calls and submit correlated responses
                let tool_results = root
                    .ai
                    .internal
                    ._run_tool_calls(calls.calls, tools, null, null, null, [], [], null, steps);
                conversation = tool_provider.submit(conversation, tool_results);
            },
            let value: T => {
                result.metadata.usage = usage;
                result.metadata.attributes.set("steps", steps);
                return root.ai.ResponseWithMetadata<T> {
                    value: value,
                    metadata: result.metadata,
                    conversation: conversation,
                };
            },
        }
    }
    throw baml.errors.UnknownError {
        data: { "provider": provider_name, "max_steps": max_steps },
        message: [provider_name + " completion exhausted its application-tool step budget"],
    }
}
