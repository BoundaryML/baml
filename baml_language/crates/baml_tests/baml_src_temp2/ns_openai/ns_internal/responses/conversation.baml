class Conversation {
    owner: root.openai.OpenAIProvider,
    history: ai.MessageHistory,
    declared_output_type: string,
    // Initial Responses input items, or function_call_output items waiting for
    // the next step.
    input: json[],
    previous_response_id: string?,
    request_url: string,
    // Non-authentication headers produced by the initial primitive request.
    // Authentication is refreshed for every continuation request.
    headers: map<string, string>,
    active_tools: json[],
    last_output: json[],

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

// Prompt tool mode keeps the task recipe because each turn renders a fresh
// T | ToolCalls output contract. It never sends OpenAI function tools.
class PromptConversation {
    owner: root.openai.OpenAIProvider,
    history: ai.MessageHistory,
    declared_output_type: string,
    recipe: ai.PromptRenderRecipe,
    identity: ai.TaskIdentity?,
    arguments: map<string, unknown>,
    tags: map<string, string>,
    // The first step renders the combined contract into these Responses input
    // items. Later steps preserve those structural items (including media) and
    // append only assistant choices and tool results.
    input: json[],
    request_url: string,
    headers: map<string, string>,
    pending_call_ids: string[],

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

// Tokens keep this portable text projection beside exact provider wire state.
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

function _headers_without_auth(headers: map<string, string>) -> map<string, string> throws never {
    let clean: map<string, string> = {};
    for (let key in headers.keys()) {
        let lower = key.to_lower_case();
        if (
            lower != "authorization"
                && lower != "api-key"
                && lower != "x-api-key"
                && lower != "content-type"
        ) {
            clean.set(key, headers[key]);
        }
    }
    clean
}

function _provider_base_request(
    provider: root.openai.OpenAIProvider,
) -> baml.http.Request throws baml.errors.UnknownError {
    let primitive = _configured_primitive(
        provider.render_shorthand(),
        provider.model,
        provider.api_key,
        provider.base_url,
        provider.extra_headers,
        provider.extra_body,
    );
    let prompt = baml.llm.assemble_prompt_ast(["Restored conversation"], []);
    let specialized = primitive.specialize_prompt(prompt) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "OpenAI Responses conversation prompt specialization failed",
            )
        },
    };
    primitive.build_request(specialized, reflect.type_of<string>()) catch (e) {
        _ => {
            throw baml.errors.UnknownError.with_message<never>(
                e,
                "OpenAI Responses conversation request build failed",
            )
        },
    }
}
