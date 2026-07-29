// Exact Gemini continuation state. `contents` retains provider-native model
// parts, including opaque thoughtSignature fields required on the next turn.
class Conversation {
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    model: string,
    history: root.ai.MessageHistory,
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
    }
}

function _headers_without_auth(headers: map<string, string>) -> map<string, string>
    throws never {
    let clean: map<string, string> = {};
    for (let key in headers.keys()) {
        let lower = key.to_lower_case();
        if (
            lower != "authorization"
                && lower != "x-goog-api-key"
                && lower != "x-goog-user-project"
        ) {
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
        _ => throw root.ai.InvalidRequest {
            provider: provider_name,
            status_code: null,
            detail: provider_name + " received another provider's conversation",
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
) -> map<string, json> throws baml.errors.UnknownError | baml.errors.Unsupported {
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
    let declarations = _agent_declarations(output_type, tools, use_sap);
    if (declarations.length() > 0) {
        body.set("tools", [{ "functionDeclarations": declarations }]);
        body.set("toolConfig", {
            "functionCallingConfig": {
                "mode": if (use_sap) { "AUTO" } else { "ANY" },
            },
        });
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
        calls.push({
            "id": id,
            "name": baml.json.path_or(raw, ".name", ""),
            "args": baml.json.path_or<json>(raw, ".args", {}),
            "raw": raw,
        });
    }
    calls
}

function _application_calls(
    calls: json[],
    raw_output: string,
    provider_name: string,
) -> root.ai.tools.ToolCalls throws root.ai.ParseFailed {
    root.ai.tools.ToolCalls {
        calls: calls.map((call: json) -> root.ai.tools.ToolCall {
            let name = baml.json.path_or(call, ".name", "");
            if (name.length() == 0 || name == _output_tool_name()) {
                throw root.ai.ParseFailed {
                    provider: provider_name,
                    raw_output: raw_output,
                };
            }
            root.ai.tools.ToolCall {
                id: baml.json.path_or(call, ".id", ""),
                name: name,
                args: baml.json.path_or<json>(call, ".args", {}),
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
) -> root.ai.tools.ModelStep<T>
    throws root.ai.Failure | baml.errors.UnknownError {
    //# Recover the exact provider-native Gemini continuation
    let state = _owned_conversation(conversation, provider_name);
    let body = _tool_body(state, reflect.type_of<T>(), tools, use_sap);
    let unauthenticated = baml.http.Request {
        method: "POST",
        url: state.url,
        headers: state.headers,
        body: baml.json.stringify(body),
    };
    //# Refresh API-key or ADC authentication without rebuilding wire state
    let request = state.primitive.authenticate_request(unauthenticated) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " continuation authentication failed",
        ),
    };
    log.info(baml.json.stringify({
        "event": "llm.tools.request",
        "provider": provider_name,
        "model": state.model,
        "content_count": state.contents.length(),
        "application_tool_count": tools.length(),
        "output_mode": if (use_sap) { "sap" } else { "native" },
        "parallel_tool_calls": parallel_tool_calls,
    }));
    //# Execute one Gemini generateContent turn
    let result = _send_json(request, provider_name);
    _validate_response(result.raw, result.text, provider_name);
    let content = _candidate_content(result.raw, result.text, provider_name);
    // Preserve all parts, not just calls/text: thoughtSignature is opaque state.
    state.contents.push(content);
    state.history.entries.push(root.ai.ChatMessage {
        message_role: root.ai.MessageRole.Assistant,
        content: [root.ai.messages.ProviderDataPart {
            provider: provider_name,
            data: content,
        }],
        metadata: null,
    });

    let raw_calls = _raw_function_calls(result.raw);
    let calls = _normalized_calls(state, raw_calls);
    let output_calls = calls.filter((call: json) -> bool {
        baml.json.path_or(call, ".name", "") == _output_tool_name()
    });
    let metadata = _response_metadata(result.raw, state.model, provider_name);

    //# Finish only from one reserved result call in native output mode
    if (!use_sap && output_calls.length() > 0) {
        if (calls.length() != 1 || output_calls.length() != 1) {
            throw root.ai.ParseFailed {
                provider: provider_name,
                raw_output: result.text,
            };
        }
        state.last_calls = [];
        return root.ai.tools.ModelStep<T> {
            outcome: _parse_output_call<T>(
                baml.json.path_or(output_calls[0], ".raw", null),
                result.text,
                provider_name,
            ),
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
        return root.ai.tools.ModelStep<T> {
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
        null => throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: result.text,
        },
    };
    state.last_calls = [];
    let value = baml.sap.parse<T>(text) catch (e) {
        _ => throw root.ai.ParseFailed {
            provider: provider_name,
            raw_output: text,
        },
    };
    root.ai.tools.ModelStep<T> {
        outcome: value,
        metadata: metadata,
    }
}

function _calls_for_result(state: Conversation, result_id: string) -> json[]
    throws never {
    state.last_calls.filter((call: json) -> bool {
        baml.json.path_or(call, ".id", "") == result_id
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
    let parts: json[] = [];
    for (let result in results) {
        let matching = _calls_for_result(state, result.id);
        if (matching.length() != 1) {
            throw root.ai.InvalidRequest {
                provider: provider_name,
                status_code: null,
                detail: "tool result has no matching Gemini function call: " + result.id,
            };
        }
        let call = matching[0];
        let response_part: json = {
            "functionResponse": {
                "id": result.id,
                "name": baml.json.path_or(call, ".name", ""),
                "response": {
                    "result": result.output,
                    "is_error": result.is_error,
                },
            },
        };
        parts.push(response_part);
        state.history.entries.push(root.ai.ChatMessage {
            message_role: root.ai.MessageRole.Tool,
            content: [root.ai.messages.ProviderDataPart {
                provider: provider_name,
                data: {
                    "id": result.id,
                    "name": baml.json.path_or(call, ".name", ""),
                    "output": result.output,
                    "is_error": result.is_error,
                },
            }],
            metadata: null,
        });
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
        {
            "role": _history_role(message.role()),
            "text": root.ai.message_text(message),
        }
    })
}

function _history_from_json(encoded: json[]) -> root.ai.MessageHistory throws never {
    root.ai.MessageHistory {
        entries: encoded.map((message: json) -> root.ai.Message {
            root.ai.ChatMessage {
                message_role: _history_role_from_string(
                    baml.json.path_or(message, ".role", "user"),
                ),
                content: [root.ai.messages.TextPart {
                    value: baml.json.path_or(message, ".text", ""),
                    metadata: {},
                }],
                metadata: null,
            }
        }),
    }
}

function _save_conversation(
    conversation: root.ai.Conversation,
    provider_name: string,
) -> root.ai.ConversationToken throws baml.errors.UnknownError {
    let state = match (conversation) {
        let value: Conversation if value.provider_name == provider_name => value,
        _ => throw baml.errors.UnknownError.with_message<never>(
            null,
            provider_name + " cannot save another provider's conversation",
        ),
    };
    root.ai.ConversationToken {
        provider: provider_name,
        version: 1,
        // Auth headers were removed when the state was created. The restored
        // primitive authenticates each continuation from current credentials.
        sealed: baml.json.stringify({
            "model": state.model,
            "history": _history_json(state.history),
            // URL, headers, query parameters, and extra_body are rebuilt from
            // the provider on restore. In particular, Vertex Express carries
            // its API key in `?key=` and must never seal that authenticated URL.
            "system_instruction": baml.json.path_or<map<string, json>?>(
                state.body_template,
                ".systemInstruction",
                null,
            ),
            "contents": state.contents,
            "last_calls": state.last_calls,
            "next_call_id": state.next_call_id,
        }),
    }
}

function _restore_conversation(
    owner: root.ai.Provider,
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
    fallback_model: string,
    token: root.ai.ConversationToken,
) -> root.ai.Conversation throws baml.errors.UnknownError {
    if (token.provider != provider_name || token.version != 1) {
        throw baml.errors.UnknownError.with_message<never>(
            token,
            provider_name + " conversation token has the wrong provider or version",
        );
    }
    let state = baml.json.parse(token.sealed) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            "invalid " + provider_name + " conversation token",
        ),
    };
    let token_model = baml.json.path_or(state, ".model", fallback_model);
    if (token_model != fallback_model) {
        throw baml.errors.UnknownError.with_message<never>(
            token_model,
            provider_name + " conversation token belongs to another model",
        );
    }
    let request = _import_base_request(primitive, provider_name);
    let body = _body_object(request, provider_name);
    match (
        baml.json.path_or<map<string, json>?>(
            state,
            ".system_instruction",
            null,
        )
    ) {
        let instruction: map<string, json> => body.set(
            "systemInstruction",
            instruction,
        ),
        null => body.delete("systemInstruction"),
    }
    let contents = baml.json.path_or<json[]>(state, ".contents", []);
    body.set("contents", contents);
    Conversation {
        owner: owner,
        primitive: primitive,
        provider_name: provider_name,
        model: fallback_model,
        history: _history_from_json(
            baml.json.path_or<json[]>(state, ".history", []),
        ),
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
            let content: json = {
                "role": role,
                "parts": [{ "text": text }],
            };
            contents.push(content);
        }
    }
    ImportedWire {
        contents: contents,
        system_parts: system_parts,
    }
}

function _import_base_request(
    primitive: baml.llm.PrimitiveClient,
    provider_name: string,
) -> baml.http.Request throws baml.errors.UnknownError {
    let prompt = baml.llm.assemble_prompt_ast(["Imported conversation"], []);
    let specialized = primitive.specialize_prompt(prompt) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " import prompt specialization failed",
        ),
    };
    primitive.build_request(specialized, reflect.type_of<string>()) catch (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            provider_name + " import request build failed",
        ),
    }
}

function _import_messages(
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
