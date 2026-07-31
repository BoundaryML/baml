class Conversation {
    owner: root.anthropic.AnthropicProvider,
    history: ai.MessageHistory,
    declared_output_type: string,
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

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }

        function messages(self) -> ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

function _provider_url(provider: root.anthropic.AnthropicProvider) -> string throws never {
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
    provider: root.anthropic.AnthropicProvider,
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

function _message_role_name(role: ai.MessageRole) -> string throws never {
    match (role) {
        ai.MessageRole.System => "system",
        ai.MessageRole.User => "user",
        ai.MessageRole.Assistant => "assistant",
        ai.MessageRole.Tool => "user",
    }
}

function _message_text(message: ai.Message) -> string throws never {
    message
        .parts()
        .map((part: ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function _append_media_mime(mime: string?) -> string throws ai.InvalidRequest {
    match (mime) {
        let value: string => value,
        null => {
            throw ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "media appended to an Anthropic conversation needs a mime type",
            }
        },
    }
}

function _append_image_block(value: image) -> json throws ai.InvalidRequest {
    match (value.url()) {
        let url: string => {
            { "type": "image", "source": { "type": "url", "url": url } }
        },
        null => {
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": _append_media_mime(value.mime_type()),
                    "data": value.base64(),
                },
            }
        },
    }
}

function _append_document_block(value: pdf) -> json throws never {
    match (value.url()) {
        let url: string => {
            { "type": "document", "source": { "type": "url", "url": url } }
        },
        null => {
            {
                "type": "document",
                "source": {
                    "type": "base64",
                    "media_type": value.mime_type() ?? "application/pdf",
                    "data": value.base64(),
                },
            }
        },
    }
}

// A media part appended mid-conversation must produce the same wire shapes
// the first-render request builder emits: image blocks with a URL or base64
// source, and document blocks for PDFs. Dispatch is on the stdlib part
// classes because the portable media union does not narrow in a match.
function _append_content_block(part: ai.MessagePart) -> json throws ai.InvalidRequest {
    match (part) {
        let image_part: ai.messages.ImagePart => _append_image_block(image_part.value),
        let pdf_part: ai.messages.PdfPart => _append_document_block(pdf_part.value),
        let audio_part: ai.messages.AudioPart => {
            throw ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "Anthropic Messages does not support audio input",
            }
        },
        let video_part: ai.messages.VideoPart => {
            throw ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "Anthropic Messages does not support video input",
            }
        },
        _ => {
            if (part.kind() != ai.MessagePartKind.Text) {
                throw ai.InvalidRequest {
                    provider: "anthropic",
                    status_code: null,
                    detail: "Anthropic append supports text, image, and PDF parts only",
                };
            }
            { "type": "text", "text": part.text() ?? "" }
        },
    }
}

// Render one validated append message into an Anthropic wire message.
// Anthropic has no mid-conversation system role, so a steering note renders
// as a user turn wrapped in a documented <steering> tag — the honest mapping.
// Consecutive user-role wire messages are fine: the Messages API combines
// same-role neighbors into one turn, so appends keep the existing convention
// of one wire message per portable message.
function _append_wire_message(message: ai.Message) -> json throws ai.InvalidRequest {
    if (message.role() == ai.MessageRole.System) {
        return {
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": "<steering>\n" + _message_text(message) + "\n</steering>",
                },
            ],
        };
    }
    {
        "role": "user",
        "content": message.parts().map((part: ai.MessagePart) -> json {
            _append_content_block(part)
        }),
    }
}

function _conversation_role_name(role: ai.MessageRole) -> string throws never {
    match (role) {
        ai.MessageRole.System => "system",
        ai.MessageRole.User => "user",
        ai.MessageRole.Assistant => "assistant",
        ai.MessageRole.Tool => "tool",
    }
}

function _conversation_role(value: string) -> ai.MessageRole throws never {
    if (value == "system") {
        return ai.MessageRole.System;
    }
    if (value == "assistant") {
        return ai.MessageRole.Assistant;
    }
    if (value == "tool") {
        return ai.MessageRole.Tool;
    }
    ai.MessageRole.User
}

// Tokens retain this portable text projection beside exact Anthropic blocks.
function _conversation_history_json(history: ai.MessageHistory) -> json[] throws never {
    history.entries.map((message: ai.Message) -> json {
        { "role": _conversation_role_name(message.role()), "text": _message_text(message) }
    })
}

function _conversation_history_from_json(encoded: json[]) -> ai.MessageHistory throws never {
    ai.MessageHistory {
        entries: encoded.map((message: json) -> ai.Message {
            ai.ChatMessage {
                message_role: _conversation_role(baml.json.path_or(message, ".role", "user")),
                content: [
                    ai.messages.TextPart {
                        value: baml.json.path_or(message, ".text", ""),
                        metadata: {},
                    },
                ],
                metadata: null,
            }
        }),
    }
}

function _continuation_headers(
    provider: root.anthropic.AnthropicProvider,
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
    provider: root.anthropic.AnthropicProvider,
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
    provider: root.anthropic.AnthropicProvider,
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
