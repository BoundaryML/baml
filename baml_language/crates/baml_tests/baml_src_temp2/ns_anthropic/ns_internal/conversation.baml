class Conversation {
    owner: root.anthropic.Messages,
    history: root.ai.MessageHistory,
    // Anthropic is stateless: every step resends these provider-native blocks.
    // Keeping them intact preserves image/document blocks, thinking signatures,
    // tool_use IDs, and other continuation data.
    system: json[],
    messages: json[],
    request_url: string,
    // Initial non-auth headers include runtime-required transport headers
    // (notably Anthropic's direct-browser opt-in on WASM).
    headers: map<string, string>,
    active_tools: json[],
    last_assistant_content: json[],

    implements root.ai.Conversation {
        function provider(self) -> root.ai.Provider throws never {
            self.owner
        }

        function messages(self) -> root.ai.Messages throws never {
            self.history
        }
    }
}

function _provider_url(provider: root.anthropic.Messages) -> string throws never {
    (provider.base_url ?? "https://api.anthropic.com") + "/v1/messages"
}

function _headers_without_auth(headers: map<string, string>) -> map<string, string> throws never {
    let clean: map<string, string> = {};
    for (let key in headers.keys()) {
        let lower = key.to_lower_case();
        if (lower != "authorization" && lower != "x-api-key") {
            clean.set(key, headers[key]);
        }
    }
    clean
}

function _provider_base_request(
    provider: root.anthropic.Messages,
) -> baml.http.Request throws baml.errors.UnknownError {
    let primitive = _configured_primitive(provider);
    let prompt = baml.llm.assemble_prompt_ast(["Restored conversation"], []);
    let specialized = primitive.specialize_prompt(prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic conversation prompt specialization failed",
            )
        },
    };
    primitive.build_request(specialized, reflect.type_of<string>()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic conversation request build failed",
            )
        },
    }
}

function _message_role_name(role: root.ai.MessageRole) -> string throws never {
    match (role) {
        root.ai.MessageRole.System => "system",
        root.ai.MessageRole.User => "user",
        root.ai.MessageRole.Assistant => "assistant",
        root.ai.MessageRole.Tool => "user",
    }
}

function _message_text(message: root.ai.Message) -> string throws never {
    message
        .parts()
        .map((part: root.ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function _continuation_headers(
    provider: root.anthropic.Messages,
    state_headers: map<string, string>,
) -> map<string, string> throws never {
    let headers: map<string, string> = {};
    for (let key in state_headers.keys()) {
        headers.set(key, state_headers[key]);
    }
    headers.set("content-type", "application/json");
    for (let key in _primitive_headers(provider).keys()) {
        headers.set(key, _primitive_headers(provider)[key]);
    }
    headers
}

function _conversation_body<T>(
    provider: root.anthropic.Messages,
    state: Conversation,
    tools: json[],
) -> map<string, json> throws baml.errors.UnknownError | baml.errors.Unsupported {
    let body: map<string, json> = {};
    for (let key in _extra_body(provider).keys()) {
        body.set(
            key,
            baml.json.to_json(_extra_body(provider)[key]) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(
                        e,
                        "Anthropic extra_body field was not JSON serializable: " + key,
                    )
                },
            },
        );
    }
    // Provider-owned fields win over extra_body.
    body.set("model", provider.model);
    body.set("system", state.system);
    body.set("messages", state.messages);
    body.set("max_tokens", provider.max_tokens);
    if (_uses_sap(provider)) {
        body.delete("output_config");
    } else {
        body.set(
            "output_config",
            { "format": { "type": "json_schema", "schema": _output_schema(reflect.type_of<T>()) } },
        );
    }
    if (tools.length() > 0) {
        body.set("tools", tools);
        body.set(
            "tool_choice",
            { "type": "auto", "disable_parallel_tool_use": !provider.parallel_tool_calls },
        );
    }
    body
}

function _authenticated_continuation_request(
    provider: root.anthropic.Messages,
    state: Conversation,
    body: map<string, json>,
) -> baml.http.Request throws baml.errors.UnknownError {
    let request = baml.http.Request {
        method: "POST",
        url: state.request_url,
        headers: _continuation_headers(provider, state.headers),
        body: baml.json.stringify(body),
    };
    _configured_primitive(provider).authenticate_request(request) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "Anthropic continuation authentication failed",
            )
        },
    }
}
