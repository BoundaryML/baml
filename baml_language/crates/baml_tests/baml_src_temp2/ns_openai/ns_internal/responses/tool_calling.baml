function _request_headers(provider: root.openai.Responses) -> map<string, string> throws never {
    let headers: map<string, string> = { "Content-Type": "application/json" };
    match (provider.api_key) {
        let key: string => {
            headers.set("Authorization", "Bearer " + key);
        },
        null => {},
    }
    match (provider.extra_headers) {
        let extra: map<string, string> => {
            for (let key in extra.keys()) {
                headers.set(key, extra[key]);
            }
        },
        null => {},
    }
    headers
}

function _request_body(
    provider: root.openai.Responses,
    input: json[],
) -> map<string, json> throws baml.errors.UnknownError {
    let body: map<string, json> = {};
    match (provider.extra_body) {
        let extra: map<string, unknown> => {
            for (let key in extra.keys()) {
                body.set(
                    key,
                    baml.json.to_json(extra[key]) catch (e) {
                        _ => {
                            throw baml.errors.UnknownError.with_message<never>(
                            e,
                            "OpenAI Responses extra_body field was not JSON serializable: " + key,
                        )
                        },
                    },
                );
            }
        },
        null => {},
    }
    // Provider-owned fields win over extra_body.
    body.set("model", provider.model);
    body.set("input", input);
    body
}

function _application_calls(
    raw_calls: json[],
    raw_output: string,
) -> root.ai.tools.ToolCalls throws root.ai.ParseFailed {
    root.ai.tools.ToolCalls {
        calls: raw_calls.map((raw: json) -> root.ai.tools.ToolCall {
            let args_text = baml.json.path<string>(raw, ".arguments") catch (e) {
                _ => {
                    throw root.ai.ParseFailed {
                    provider: "openai-responses",
                    raw_output: raw_output,
                }
                },
            };
            let args = baml.json.parse(args_text) catch (e) {
                _ => {
                    throw root.ai.ParseFailed {
                    provider: "openai-responses",
                    raw_output: raw_output,
                }
                },
            };
            root.ai.tools.ToolCall {
                id: baml.json.path_or(raw, ".call_id", baml.json.path_or(raw, ".id", "")),
                name: baml.json.path_or(raw, ".name", ""),
                args: args,
            }
        }),
    }
}

function _allow_parallel_tool_calls(
    provider: root.openai.Responses,
    application_tool_count: int,
) -> bool throws never {
    provider.parallel_tool_calls && !_is_strict(provider) && application_tool_count > 1
}
