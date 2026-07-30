function _responses_built_request<T>(
    provider: root.openai.OpenAIProvider,
    task: ai.Task<T>,
    background: bool,
    log_shape: bool = true,
) -> baml.http.Request throws ai.Failure | baml.errors.UnknownError {
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
        // A typed Agent turn always expects exactly one final result call.
        // Output cardinality is represented inside value's schema.
        body.set("parallel_tool_calls", false);
    } else {
        // SAP output relies on the task's ctx.output_format instruction
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
