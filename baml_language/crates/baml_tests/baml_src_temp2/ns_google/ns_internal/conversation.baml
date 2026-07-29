// Exact Gemini continuation state. `contents` retains provider-native model
// parts, including opaque thoughtSignature fields required on the next turn.
class Conversation {
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    history: root.ai.MessageHistory,
    declared_output_type: string,
    url: string,
    headers: map<string, string>,
    body_template: json,
    contents: json[],
    last_calls: json[],
    next_call_id: int,

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

// Prompt tool mode keeps the provider's task recipe so each turn can render
// one text contract for T | ToolCalls. It intentionally stores no native
// Gemini function-call state.
class PromptConversation {
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    history: root.ai.MessageHistory,
    declared_output_type: string,
    recipe: root.ai.PromptRenderRecipe,
    identity: root.ai.TaskIdentity?,
    arguments: map<string, unknown>,
    tags: map<string, string>,
    last_calls: root.ai.tools.ToolCall[],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }
        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

function _prompt_tool_schema(tool: root.ai.tools.Tool) -> json throws never {
    { "name": tool.name, "description": tool.description, "parameters": tool.input_schema }
}

function _prompt_tool_output_suffix(tools: root.ai.tools.Tool[]) -> string throws never {
    let schemas = tools.map((tool: root.ai.tools.Tool) -> json {
        _prompt_tool_schema(tool)
    });
    "\n\n"
        + `
            The output contract above permits either the final result or a ToolCalls value.
            To use tools, return ToolCalls with one or more calls. Each call must have a
            unique id, an exact tool name from the catalog, and args matching that tool's
            parameters JSON Schema. Do not invent tool names. After tool results are
            provided, choose tools again or return the final result.

            Available tools:
            ${baml.json.stringify(schemas)}
        `
}

function _prompt_message_text(message: root.ai.Message) -> string throws never {
    message
        .parts()
        .map((part: root.ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function _prompt_role(role: root.ai.MessageRole) -> string throws never {
    match (role) {
        root.ai.MessageRole.System => "system",
        root.ai.MessageRole.User => "user",
        root.ai.MessageRole.Assistant => "assistant",
        root.ai.MessageRole.Tool => "tool",
    }
}

function _prompt_history(history: root.ai.MessageHistory) -> string throws never {
    if (history.entries.length() <= 1) {
        return "";
    }
    history
        .entries
        .slice(1, history.entries.length())
        .map((message: root.ai.Message) -> string {
            _prompt_role(message.role()) + ": " + _prompt_message_text(message)
        })
        .join("\n")
}

function _new_prompt_conversation<T>(
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    task: root.ai.Task<T>,
) -> PromptConversation throws never {
    PromptConversation {
        owner: owner,
        primitive: primitive,
        provider_name: provider_name,
        model: model,
        history: task.messages().to_history(),
        declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
        recipe: task._render,
        identity: task.identity,
        arguments: task.arguments,
        tags: task.tags,
        last_calls: [],
    }
}

function _owned_prompt_conversation(
    conversation: root.ai.Conversation,
    provider_name: string,
) -> PromptConversation throws root.ai.InvalidRequest {
    let state = match (conversation) {
        let value: PromptConversation => value,
        _ => {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: provider_name + " prompt tools received another provider's conversation",
            }
        },
    };
    if (state.provider_name != provider_name) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: provider_name + " prompt tools received another Google backend's conversation",
        };
    }
    state
}

function _prompt_turn<T>(
    state: PromptConversation,
    tools: root.ai.tools.Tool[],
    output_suffix: string,
) -> root.ai.Task<T> throws never {
    //# Render the task with the text choice for this turn
    let rendered = state.recipe.render_with_output_format(state.owner, reflect.type_of<T>(), output_suffix);
    //# Reattach prior tool-loop messages after the original rendered prompt
    let prior = _prompt_history(state.history);
    let prompt = if (prior.length() == 0) {
        rendered
    } else {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n\nConversation so far:\n", ""],
                [root.ai.messages._prompt_text_content(rendered), prior],
            )
    };
    let recipe = root.ai.PromptRenderRecipe {
        template: (ctx: baml.llm.Context) -> baml.llm.PromptAst {
            prompt
        },
    };
    root.ai.Task<T> {
        provider: state.owner,
        prompt: prompt,
        identity: state.identity,
        arguments: state.arguments,
        tools: tools,
        tags: state.tags,
        _render: recipe,
    }
}

function _prompt_step<T>(
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
    provider_name: string,
) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Resume the Google text-tool conversation
    let state = _owned_prompt_conversation(conversation, provider_name);
    if (state.last_calls.length() != 0) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "submit tool results before requesting the next Google prompt-tool step",
        };
    }
    //# A zero-tool turn asks for T directly and parses it with SAP
    if (tools.length() == 0) {
        let response = _generate<T>(
                state.primitive,
                _prompt_turn<T>(state, [], ""),
                provider_name,
                state.model,
                true,
            );
        state.last_calls = [];
        state
            .history
            .entries
            .push(root.ai.ChatMessage.assistant(_response_text(response.metadata.raw) ?? ""));
        return root.ai.ModelStep<T> { outcome: response.value, metadata: response.metadata };
    }
    //# Ask for the text choice T | ToolCalls without native function declarations
    let response = _generate<T | root.ai.tools.ToolCalls>(
            state.primitive,
            _prompt_turn<T | root.ai.tools.ToolCalls>(
                    state,
                    tools,
                    _prompt_tool_output_suffix(tools),
                ),
            provider_name,
            state.model,
            true,
        );
    match (response.value) {
        let calls: root.ai.tools.ToolCalls => {
            state.last_calls = calls.calls;
            state
                .history
                .entries
                .push(
                    root.ai
                        .ChatMessage
                        .assistant(
                            baml.json
                                .stringify(
                                    {
                                        "calls": calls.calls.map((call: root.ai.tools.ToolCall) -> json {
                                            { "id": call.id, "name": call.name, "args": call.args }
                                        }),
                                    },
                                ),
                        ),
                );
            root.ai.ModelStep<T> { outcome: calls, metadata: response.metadata }
        },
        let value: T => {
            state.last_calls = [];
            state
                .history
                .entries
                .push(root.ai.ChatMessage.assistant(_response_text(response.metadata.raw) ?? ""));
            root.ai.ModelStep<T> { outcome: value, metadata: response.metadata }
        },
    }
}

function _prompt_results_for_call(
    results: root.ai.tools.ToolResult[],
    call_id: string,
) -> root.ai.tools.ToolResult[] throws never {
    results.filter((result: root.ai.tools.ToolResult) -> bool {
        result.id == call_id
    })
}

function _prompt_submit(
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
    provider_name: string,
) -> root.ai.Conversation throws root.ai.Failure {
    //# Correlate every text-protocol result before mutating the transcript
    let state = _owned_prompt_conversation(conversation, provider_name);
    if (results.length() != state.last_calls.length()) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "tool results must match every pending Google prompt-tool call",
        };
    }
    for (let call in state.last_calls) {
        let pending_count = state
            .last_calls
            .filter((candidate: root.ai.tools.ToolCall) -> bool {
                candidate.id == call.id
            })
            .length();
        let result_count = _prompt_results_for_call(results, call.id).length();
        if (call.id.length() == 0 || pending_count != 1 || result_count != 1) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: `tool results must contain exactly one result for each pending Google prompt-tool call (${call.id}: pending=${pending_count}, results=${result_count})`,
            };
        }
    }
    //# Append results in the model's call order
    for (let call in state.last_calls) {
        let result = _prompt_results_for_call(results, call.id)[0];
        state
            .history
            .entries
            .push(
                root.ai.ChatMessage {
                    message_role: root.ai.MessageRole.Tool,
                    content: [
                        root.ai.messages.TextPart {
                            value: baml
                                .json
                                .stringify(
                                    {
                                        "id": result.id,
                                        "output": result.output,
                                        "is_error": result.is_error,
                                    },
                                ),
                            metadata: {},
                        },
                    ],
                    metadata: null,
                },
            );
    }
    state.last_calls = [];
    state
}

function _headers_without_auth(headers: map<string, string>) -> map<string, string> throws never {
    let clean: map<string, string> = {};
    for (let key in headers.keys()) {
        let lower = key.to_lower_case();
        if (lower != "authorization" && lower != "x-goog-api-key" && lower != "x-goog-user-project") {
            clean.set(key, headers[key]);
        }
    }
    clean
}

function _new_conversation<T>(
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    task: root.ai.Task<T>,
) -> Conversation throws baml.errors.UnknownError {
    let request = _built_request(primitive, task, provider_name);
    let body = _body_object(request, provider_name);
    Conversation {
        owner: owner,
        primitive: primitive,
        provider_name: provider_name,
        model: model,
        history: task.messages().to_history(),
        declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
        url: request.url,
        headers: _headers_without_auth(request.headers),
        body_template: body,
        contents: baml.json.path_or<json[]>(body, ".contents", []),
        last_calls: [],
        next_call_id: 1,
    }
}

function _owned_conversation(
    conversation: root.ai.Conversation,
    provider_name: string,
) -> Conversation throws root.ai.InvalidRequest {
    let state = match (conversation) {
        let value: Conversation => value,
        _ => {
            throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: provider_name + " received another provider's conversation",
        }
        },
    };
    if (state.provider_name != provider_name) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: provider_name + " received another Google backend's conversation",
        };
    }
    state
}

function _tool_body(
    state: Conversation,
    output_type: type,
    tools: root.ai.tools.Tool[],
    use_sap: bool,
) -> map<string, json> throws baml.errors.UnknownError | baml.errors.Unsupported | root.ai.InvalidRequest {
    let body = _clone_body(state.body_template, state.provider_name);
    body.set("contents", state.contents);
    // Native generation schemas and function calling are mutually exclusive.
    match (body.get("generationConfig")) {
        let config: map<string, json> => {
            config.delete("responseMimeType");
            config.delete("responseJsonSchema");
            config.delete("responseSchema");
            body.set("generationConfig", config);
        },
        _ => {},
    }
    body.delete("tools");
    body.delete("toolConfig");
    let declarations = _agent_declarations(output_type, tools, use_sap, state.provider_name);
    if (declarations.length() > 0) {
        body.set("tools", [{ "functionDeclarations": declarations }]);
        body.set(
            "toolConfig",
            {
                "functionCallingConfig": {
                    "mode": if (use_sap) {
                        "AUTO"
                    } else {
                        "ANY"
                    },
                },
            },
        );
    }
    body
}

function _normalized_calls(state: Conversation, raw_calls: json[]) -> json[] throws never {
    let calls: json[] = [];
    for (let raw in raw_calls) {
        let id = match (baml.json.path_or<string?>(raw, ".id", null)) {
            let existing: string if existing.length() > 0 => existing,
            _ => {
                let generated = "google_call_" + state.next_call_id.to_string();
                state.next_call_id = state.next_call_id + 1;
                generated
            },
        };
        calls
            .push(
                {
                    "id": id,
                    "name": baml.json.path_or(raw, ".name", ""),
                    "args": baml.json.path_or<json>(raw, ".args", {}),
                    "raw": raw,
                },
            );
    }
    calls
}

function _normalized_call_object(call: json) -> map<string, json> throws never {
    baml.json.from_json<map<string, json>>(call) catch (e) {
        _ => {
            let empty: map<string, json> = {};
            empty
        },
    }
}

function _normalized_call_id(call: json) -> string throws never {
    match (_normalized_call_object(call).get("id")) {
        let value: string => value,
        _ => "",
    }
}

function _normalized_call_name(call: json) -> string throws never {
    match (_normalized_call_object(call).get("name")) {
        let value: string => value,
        _ => "",
    }
}

function _normalized_call_args(call: json) -> json throws never {
    match (_normalized_call_object(call).get("args")) {
        let value: map<string, json> => value,
        _ => {
            let empty: map<string, json> = {};
            empty
        },
    }
}

function _normalized_call_raw(call: json) -> json throws never {
    match (_normalized_call_object(call).get("raw")) {
        let value: map<string, json> => value,
        _ => {
            let empty: map<string, json> = {};
            empty
        },
    }
}

function _application_calls(
    calls: json[],
    raw_output: string,
    provider_name: string,
) -> root.ai.tools.ToolCalls throws root.ai.ParseFailed {
    root.ai.tools.ToolCalls {
        calls: calls.map((call: json) -> root.ai.tools.ToolCall {
            let name = _normalized_call_name(call);
            if (name.length() == 0 || name == _output_tool_name()) {
                throw root.ai.ParseFailed {
                    provider: provider_name,
                    raw_output: raw_output,
                };
            }
            root.ai.tools.ToolCall {
                id: _normalized_call_id(call),
                name: name,
                args: _normalized_call_args(call),
            }
        }),
    }
}

function _step<T>(
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
    provider_name: string,
    use_sap: bool,
    parallel_tool_calls: bool,
) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    //# Recover the exact provider-native Gemini continuation
    let state = _owned_conversation(conversation, provider_name);
    if (state.last_calls.length() != 0) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "submit tool results before requesting the next Gemini step",
        };
    }
    // With no application tools, Gemini's native JSON response schema is the
    // strongest typed-output contract. Once tools are present, native mode uses
    // the reserved result function so T and application calls share one choice.
    let body = if (tools.length() == 0) {
        let generation = _clone_body(state.body_template, state.provider_name);
        generation.set("contents", state.contents);
        _generation_body(generation, reflect.type_of<T>(), use_sap)
    } else {
        _tool_body(state, reflect.type_of<T>(), tools, use_sap)
    };
    let unauthenticated = baml.http.Request {
        method: "POST",
        url: state.url,
        headers: state.headers,
        body: baml.json.stringify(body),
    };
    //# Refresh API-key or ADC authentication without rebuilding wire state
    let request = state.primitive.authenticate_request(unauthenticated) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " continuation authentication failed",
        )
        },
    };
    log.info(
        baml.json
            .stringify(
                {
                    "event": if (tools.length() == 0) {
                        "llm.request"
                    } else {
                        "llm.tools.request"
                    },
                    "provider": provider_name,
                    "model": state.model,
                    "output_type": state.declared_output_type,
                    "content_count": state.contents.length(),
                    "application_tool_count": tools.length(),
                    "output_mode": if (use_sap) {
                        "sap"
                    } else {
                        "native"
                    },
                    "parallel_tool_calls": parallel_tool_calls,
                },
            ),
    );
    //# Execute one Gemini generateContent turn
    let result = _send_json(request, provider_name);
    _validate_response(result.raw, result.text, provider_name);
    let content = _candidate_content(result.raw, result.text, provider_name);
    // Preserve all parts, not just calls/text: thoughtSignature is opaque state.
    state.contents.push(content);
    state
        .history
        .entries
        .push(
            root.ai.ChatMessage {
                message_role: root.ai.MessageRole.Assistant,
                content: [root.ai.messages.ProviderDataPart { provider: provider_name, data: content }],
                metadata: null,
            },
        );

    let raw_calls = _raw_function_calls(result.raw);
    let calls = _normalized_calls(state, raw_calls);
    let output_calls = calls.filter((call: json) -> bool {
        _normalized_call_name(call) == _output_tool_name()
    });
    let metadata = _response_metadata(result.raw, state.model, provider_name);
    //# Finish a zero-tool turn through native JSON schema or SAP text parsing
    if (tools.length() == 0) {
        state.last_calls = [];
        return root.ai.ModelStep<T> {
            outcome: _parse_generation_value<T>(result.raw, result.text, provider_name, use_sap),
            metadata: metadata,
        };
    }
    //# Finish only from one reserved result call in native output mode
    if (!use_sap && output_calls.length() > 0) {
        if (calls.length() != 1 || output_calls.length() != 1) {
            throw root.ai.ParseFailed {
                provider: provider_name,
                raw_output: result.text,
            };
        }
        state.last_calls = [];
        return root.ai.ModelStep<T> {
            outcome: _parse_output_call<T>(_normalized_call_raw(output_calls[0]), result.text, provider_name),
            metadata: metadata,
        };
    }
    //# Yield application calls to the runner
    if (calls.length() > 0) {
        if (!parallel_tool_calls && calls.length() > 1) {
            throw root.ai.ParseFailed {
                provider: provider_name,
                raw_output: result.text,
            };
        }
        state.last_calls = calls;
        return root.ai.ModelStep<T> {
            outcome: _application_calls(calls, result.text, provider_name),
            metadata: metadata,
        };
    }
    //# Hybrid SAP mode accepts final T from ordinary text
    if (!use_sap) {
        throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: result.text,
        };
    }
    let text = match (_response_text(result.raw)) {
        let value: string => value,
        null => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: result.text,
        }
        },
    };
    state.last_calls = [];
    let value = baml.sap.parse<T>(text) catch (e) {
        _ => {
            throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: text,
        }
        },
    };
    root.ai.ModelStep<T> { outcome: value, metadata: metadata }
}

function _calls_for_result(state: Conversation, result_id: string) -> json[] throws never {
    state.last_calls.filter((call: json) -> bool {
        _normalized_call_id(call) == result_id
    })
}

function _results_for_call(
    results: root.ai.tools.ToolResult[],
    call_id: string,
) -> root.ai.tools.ToolResult[] throws never {
    results.filter((result: root.ai.tools.ToolResult) -> bool {
        result.id == call_id
    })
}

function _submit(
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
    provider_name: string,
) -> root.ai.Conversation throws root.ai.Failure {
    //# Correlate every result with the function call Gemini emitted
    let state = _owned_conversation(conversation, provider_name);
    if (results.length() != state.last_calls.length()) {
        throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: "tool results must match every pending Gemini function call",
        };
    }
    // Validate the whole result set before mutating conversation/history.
    // Each pending provider call ID and each submitted result ID must occur
    // exactly once. A duplicate id1 cannot satisfy pending id1 + id2.
    for (let call in state.last_calls) {
        let call_id = _normalized_call_id(call);
        let pending_count = _calls_for_result(state, call_id).length();
        let result_count = _results_for_call(results, call_id).length();
        if (call_id.length() == 0 || pending_count != 1 || result_count != 1) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: `tool results must contain exactly one result for each pending Gemini call (${call_id}: pending=${pending_count}, results=${result_count})`,
            };
        }
    }
    let parts: json[] = [];
    // Emit in the model/provider call order, independent of runner result order.
    for (let call in state.last_calls) {
        let call_id = _normalized_call_id(call);
        let result = _results_for_call(results, call_id)[0];
        let response_part: json = {
            "functionResponse": {
                "id": call_id,
                "name": _normalized_call_name(call),
                "response": { "result": result.output, "is_error": result.is_error },
            },
        };
        parts.push(response_part);
        state
            .history
            .entries
            .push(
                root.ai.ChatMessage {
                    message_role: root.ai.MessageRole.Tool,
                    content: [
                        root.ai.messages.ProviderDataPart {
                            provider: provider_name,
                            data: {
                                "id": call_id,
                                "name": _normalized_call_name(call),
                                "output": result.output,
                                "is_error": result.is_error,
                            },
                        },
                    ],
                    metadata: null,
                },
            );
    }
    state.contents.push({ "role": "user", "parts": parts });
    state.last_calls = [];
    state
}

function _history_role(role: root.ai.MessageRole) -> string throws never {
    match (role) {
        root.ai.MessageRole.System => "system",
        root.ai.MessageRole.User => "user",
        root.ai.MessageRole.Assistant => "assistant",
        root.ai.MessageRole.Tool => "tool",
    }
}

function _history_role_from_string(role: string) -> root.ai.MessageRole throws never {
    if (role == "system") {
        return root.ai.MessageRole.System;
    }
    if (role == "assistant") {
        return root.ai.MessageRole.Assistant;
    }
    if (role == "tool") {
        return root.ai.MessageRole.Tool;
    }
    root.ai.MessageRole.User
}

function _history_json(history: root.ai.MessageHistory) -> json[] throws never {
    history.entries.map((message: root.ai.Message) -> json {
        { "role": _history_role(message.role()), "text": root.ai.message_text(message) }
    })
}

function _history_from_json(encoded: json[]) -> root.ai.MessageHistory throws never {
    root.ai.MessageHistory {
        entries: encoded.map((message: json) -> root.ai.Message {
            root.ai.ChatMessage {
                message_role: _history_role_from_string(baml.json.path_or(message, ".role", "user")),
                content: [
                    root.ai.messages.TextPart {
                        value: baml.json.path_or(message, ".text", ""),
                        metadata: {},
                    },
                ],
                metadata: null,
            }
        }),
    }
}

function _save_conversation(
    conversation: root.ai.Conversation,
    owner: root.ai.Provider,
    provider_name: string,
    provider_identity: string,
) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
        let value: Conversation if value.provider_name == provider_name => value,
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            null,
            provider_name + " cannot save another provider's conversation",
        )
        },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, owner)) {
        throw baml.errors.UnknownError.with_message<never>(
            null,
            provider_name + " cannot save a conversation owned by another provider instance",
        )
    }
    root.ai.ConversationToken {
        provider: provider_name,
        version: 1,
        // Auth headers were removed when the state was created. The restored
        // primitive authenticates each continuation from current credentials.
        sealed: baml
            .json
            .stringify(
                {
                    "model": state.model,
                    "provider_identity": provider_identity,
                    "output_type": state.declared_output_type,
                    "history": _history_json(state.history),
                    // URL, headers, query parameters, and extra_body are rebuilt from
                    // the provider on restore. In particular, Vertex Express carries
                    // its API key in `?key=` and must never seal that authenticated URL.
                    "system_instruction": baml
                        .json
                        .path_or<map<string, json>?>(
                            state.body_template,
                            ".systemInstruction",
                            null,
                        ),
                    "contents": state.contents,
                    "last_calls": state.last_calls,
                    "next_call_id": state.next_call_id,
                },
            ),
    }
}

function _restore_conversation(
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    fallback_model: string,
    provider_identity: string,
    token: root.ai.ConversationToken,
) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (token.provider != provider_name || token.version != 1) {
        throw baml.errors.UnknownError.with_message<never>(
            token,
            provider_name + " conversation token has the wrong provider or version",
        );
    }
    let state = baml.json.parse(token.sealed) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            "invalid " + provider_name + " conversation token",
        )
        },
    };
    let token_model = baml.json.path_or(state, ".model", fallback_model);
    if (token_model != fallback_model) {
        throw baml.errors.UnknownError.with_message<never>(
            token_model,
            provider_name + " conversation token belongs to another model",
        );
    }
    if (baml.json.path_or(state, ".provider_identity", "") != provider_identity) {
        throw baml.errors.UnknownError.with_message<never>(
            token,
            provider_name + " conversation token belongs to another provider configuration",
        )
    }
    let output_type = baml.json.path_or(state, ".output_type", "");
    if (output_type.length() == 0) {
        throw baml.errors.UnknownError.with_message<never>(
            token,
            provider_name + " conversation token has no output type",
        );
    }
    let request = _import_base_request(primitive, provider_name);
    let body = _body_object(request, provider_name);
    match (baml.json.path_or<map<string, json>?>(state, ".system_instruction", null)) {
        let instruction: map<string, json> => body.set("systemInstruction", instruction),
        null => body.delete("systemInstruction"),
    }
    let contents = baml.json.path_or<json[]>(state, ".contents", []);
    body.set("contents", contents);
    Conversation {
        owner: owner,
        primitive: primitive,
        provider_name: provider_name,
        model: fallback_model,
        history: _history_from_json(baml.json.path_or<json[]>(state, ".history", [])),
        declared_output_type: output_type,
        url: request.url,
        headers: _headers_without_auth(request.headers),
        body_template: body,
        contents: contents,
        last_calls: baml.json.path_or<json[]>(state, ".last_calls", []),
        next_call_id: baml.json.path_or(state, ".next_call_id", 1),
    }
}

class ImportedWire {
    contents: json[],
    system_parts: json[],
}

function _messages_wire(messages: root.ai.Messages) -> ImportedWire throws never {
    let contents: json[] = [];
    let system_parts: json[] = [];
    for (let message in messages.items()) {
        let text = root.ai.message_text(message);
        if (message.role() == root.ai.MessageRole.System) {
            let part: json = { "text": text };
            system_parts.push(part);
        } else {
            let role = if (message.role() == root.ai.MessageRole.Assistant) {
                "model"
            } else {
                "user"
            };
            let content: json = { "role": role, "parts": [{ "text": text }] };
            contents.push(content);
        }
    }
    ImportedWire { contents: contents, system_parts: system_parts }
}

function _import_base_request(
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
) -> baml.http.Request throws baml.errors.UnknownError {
    let prompt = baml.llm.assemble_prompt_ast(["Imported conversation"], []);
    let specialized = primitive.specialize_prompt(prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " import prompt specialization failed",
        )
        },
    };
    primitive.build_request(specialized, reflect.type_of<string>()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " import request build failed",
        )
        },
    }
}

function _import_messages<T>(
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    messages: root.ai.Messages,
) -> root.ai.ConversationImport throws baml.errors.UnknownError {
    let request = _import_base_request(primitive, provider_name);
    let body = _body_object(request, provider_name);
    let wire = _messages_wire(messages);
    body.set("contents", wire.contents);
    if (wire.system_parts.length() > 0) {
        body.set("systemInstruction", { "parts": wire.system_parts });
    } else {
        body.delete("systemInstruction");
    }
    root.ai.ConversationImport {
        conversation: Conversation {
            owner: owner,
            primitive: primitive,
            provider_name: provider_name,
            model: model,
            history: messages.to_history(),
            declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
            url: request.url,
            headers: _headers_without_auth(request.headers),
            body_template: body,
            contents: wire.contents,
            last_calls: [],
            next_call_id: 1,
        },
        fidelity: root.ai.ConversationFidelity.MessagesOnly,
        warnings: [
            "message import keeps text and roles but cannot reconstruct provider thought signatures or media parts",
        ],
    }
}
