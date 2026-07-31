function _request_headers(provider: root.openai.OpenAIProvider) -> map<string, string> throws never {
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

function _authenticated_request(
    provider: root.openai.OpenAIProvider,
    method: string,
    url: string,
    headers: map<string, string>,
    body: string,
) -> baml.http.Request throws baml.errors.UnknownError {
    let normalized_headers = _headers_without_auth(headers);
    normalized_headers.set("Content-Type", "application/json");
    let request = baml.http.Request { method: method, url: url, headers: normalized_headers, body: body };
    _configured_primitive(
        provider.render_shorthand(),
        provider.model,
        provider.api_key,
        provider.base_url,
        provider.extra_headers,
        provider.extra_body,
    )
    .authenticate_request(request) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "OpenAI Responses continuation authentication failed",
            )
        },
    }
}

function _authenticated_continuation_request(
    provider: root.openai.OpenAIProvider,
    state: Conversation,
    body: map<string, json>,
) -> baml.http.Request throws baml.errors.UnknownError {
    let headers: map<string, string> = {};
    for (let key in state.headers.keys()) {
        headers.set(key, state.headers[key]);
    }
    _authenticated_request(provider, "POST", state.request_url, headers, baml.json.stringify(body))
}

function _request_body(
    provider: root.openai.OpenAIProvider,
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
) -> ai.tools.ToolCalls throws ai.ParseFailed {
    ai.tools.ToolCalls {
        calls: raw_calls.map((raw: json) -> ai.tools.ToolCall {
            let args_text = baml.json.path<string>(raw, ".arguments") catch (e) {
                _ => {
                    throw ai.ParseFailed {
                    provider: "openai-responses",
                    raw_output: raw_output,
                }
                },
            };
            let args = baml.json.parse(args_text) catch (e) {
                _ => {
                    throw ai.ParseFailed {
                    provider: "openai-responses",
                    raw_output: raw_output,
                }
                },
            };
            ai.tools.ToolCall {
                id: baml.json.path_or(raw, ".call_id", baml.json.path_or(raw, ".id", "")),
                name: baml.json.path_or(raw, ".name", ""),
                args: args,
            }
        }),
    }
}

function _allow_parallel_tool_calls(
    provider: root.openai.OpenAIProvider,
    application_tool_count: int,
) -> bool throws never {
    provider.parallel_tool_calls && !_is_strict(provider) && application_tool_count > 1
}

function _pending_call_ids(raw_calls: json[]) -> string[] throws ai.InvalidRequest {
    let ids: string[] = [];
    for (let call in raw_calls) {
        let id = baml.json.path_or(call, ".call_id", baml.json.path_or(call, ".id", ""));
        if (id.length() == 0 || ids.includes(id)) {
            throw ai.InvalidRequest {
                provider: "openai-responses",
                status_code: null,
                detail: "pending OpenAI tool calls must have unique, non-empty call IDs",
            };
        }
        ids.push(id);
    }
    ids
}

function _correlate_tool_results(
    pending_ids: string[],
    results: ai.tools.ToolResult[],
) -> ai.tools.ToolResult[] throws ai.InvalidRequest {
    if (pending_ids.length() == 0 || pending_ids.length() != results.length()) {
        throw ai.InvalidRequest {
            provider: "openai-responses",
            status_code: null,
            detail: "tool results must match every pending OpenAI function call",
        };
    }
    let ordered: ai.tools.ToolResult[] = [];
    for (let call_id in pending_ids) {
        let matching = results.filter((result: ai.tools.ToolResult) -> bool {
            ai.tools.result_id(result) == call_id
        });
        if (matching.length() != 1) {
            throw ai.InvalidRequest {
                provider: "openai-responses",
                status_code: null,
                detail: "tool result IDs must match pending OpenAI call IDs exactly once",
            };
        }
        ordered.push(matching[0]);
    }
    ordered
}
