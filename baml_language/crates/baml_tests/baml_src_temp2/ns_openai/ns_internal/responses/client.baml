function _responses_built_request<T>(
    provider: root.openai.Responses,
    task: root.ai.Task<T>,
    background: bool,
    log_shape: bool = true,
) -> baml.http.Request throws root.ai.Failure | baml.errors.UnknownError {
    _validate_output_config(provider);
    let strict = _is_strict(provider);
    let use_sap = _uses_sap(provider);
    let primitive = _configured_primitive(
        provider.render_shorthand(),
        provider.model,
        provider.api_key,
        provider.base_url,
        provider.extra_headers,
        provider.extra_body,
    );
    let specialized = primitive.specialize_prompt(task.prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "OpenAI Responses prompt specialization failed",
        )
        },
    };
    let built = primitive.build_request(specialized, task.output_type()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "OpenAI Responses request build failed",
        )
        },
    };
    let parsed = baml.json.parse(built.body) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "OpenAI Responses request body was not JSON",
        )
        },
    };
    let body = match (parsed) {
        let object: map<string, json> => object,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            parsed,
            "OpenAI Responses request body was not an object",
        )
        },
    };
    if (!use_sap) {
        body.set("tools", [_output_tool(task.output_type(), strict)]);
        body.set("tool_choice", { "type": "function", "name": _output_tool_name() });
        // A bounded typed completion always expects exactly one result call.
        // Output cardinality is represented inside value's schema.
        body.set("parallel_tool_calls", false);
    } else {
        // SAP completion relies on the task's ctx.output_format instruction
        // and parses output_text as T. It sends no synthetic result tool.
        // Prompt tool mode may deliberately make that T be T | ToolCalls when
        // the caller wants a completely text-based agent protocol.
    }
    if (background) {
        body.set("background", true);
        body.set("store", true);
    }
    if (log_shape) {
        log.info(
            baml.json
                .stringify(
                    {
                        "event": "llm.request.shape",
                        "provider": "openai-responses",
                        "model": provider.model,
                        "input_count": baml.json.path_or<json[]>(body, ".input", []).length(),
                        "tool_names": baml
                            .json
                            .path_or<json[]>(body, ".tools", [])
                            .map((tool: json) -> string {
                                baml.json.path_or(tool, ".name", "")
                            })
                            .join(","),
                        "tool_choice": baml.json.path_or<json?>(body, ".tool_choice", null),
                        "parallel_tool_calls": baml.json.path_or<bool?>(body, ".parallel_tool_calls", null),
                        "output_mode": provider.output_mode.to_string(),
                        "tool_mode": provider.tool_mode.to_string(),
                        "background": background,
                    },
                ),
        );
    }
    baml.http.Request {
        method: built.method,
        url: built.url,
        headers: built.headers,
        body: baml.json.stringify(body),
    }
}

function _responses_generate<T>(
    provider: root.openai.Responses,
    task: root.ai.Task<T>,
) -> root.ai.ResponseWithMetadata<T> throws root.ai.Failure | baml.errors.UnknownError {
    let request = _responses_built_request(provider, task, false);
    log.info(
        baml.json
            .stringify(
                {
                    "event": "llm.request",
                    "provider": "openai-responses",
                    "model": provider.model,
                    "request_bytes": request.body.length(),
                    "output_mode": provider.output_mode.to_string(),
                    "tool_mode": provider.tool_mode.to_string(),
                },
            ),
    );
    let response = baml.http.send(request) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "OpenAI Responses send failed",
        )
        },
    };
    let text = response.text() catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "OpenAI Responses body read failed",
        )
        },
    };
    log.info(
        baml.json
            .stringify(
                {
                    "event": "llm.response",
                    "provider": "openai-responses",
                    "model": provider.model,
                    "status_code": response.status_code,
                    "response_bytes": text.length(),
                },
            ),
    );
    if (!response.ok()) {
        throw root.ai.internal.classify_http(
            "openai-responses",
            response.status_code,
            text,
        );
    }
    let raw = baml.json.parse(text) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: "openai-responses",
            raw_output: text,
        }
        },
    };
    root.ai.ResponseWithMetadata<T> {
        value: _parse_response_value<T>(raw, text, _uses_sap(provider)),
        metadata: _response_metadata(raw, provider.model),
        conversation: null,
    }
}
