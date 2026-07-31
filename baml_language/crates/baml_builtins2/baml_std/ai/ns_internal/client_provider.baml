// Adapter used by compiler-generated `client:` sugar. It gives an existing
// baml.llm.Client the same Provider + AgentProvider lifecycle as a provider
// object, including prompt-based application tools.
class ClientProvider {
    client: baml.llm.Client,
}

class ClientConversation {
    owner: ClientProvider,
    history: ai.MessageHistory,
    initial_history_len: int,
    declared_output_type: string,
    recipe: ai.PromptRenderRecipe,
    identity: ai.TaskIdentity?,
    arguments: map<string, unknown>,
    tags: map<string, string>,
    pending: ai.tools.ToolCall[],

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

        function pending_calls(self) -> ai.tools.ToolCall[]? throws never {
            self.pending
        }
    }
}

function _render_primitive(provider: ClientProvider) -> baml.llm.PrimitiveClient throws never {
    provider.client.to_primitive_client(lenient = true) catch (e) {
        _ => baml.sys.panic("ai: client could not be resolved for prompt rendering")
    }
}

implements ai.Provider for ClientProvider {
    function name(self) -> string throws never {
        self.client.name
    }

    function render_shorthand(self) -> string throws never {
        let primitive = _render_primitive(self);
        primitive.provider + "/" + (primitive.options.model ?? primitive.name)
    }

    function _render_client(self) -> baml.llm.PrimitiveClient throws never {
        _render_primitive(self)
    }
}

function _client_tool_schema(tool: ai.tools.Tool) -> json throws never {
    {
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    }
}

function _client_tool_output_suffix(tools: ai.tools.Tool[]) -> string throws never {
    let schemas = tools.map((tool: ai.tools.Tool) -> json {
        _client_tool_schema(tool)
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

function _client_message_text(message: ai.Message) -> string throws never {
    message
        .parts()
        .map((part: ai.MessagePart) -> string {
            part.text() ?? ""
        })
        .join("")
}

function _client_message_role(role: ai.MessageRole) -> string throws never {
    match (role) {
        ai.MessageRole.System => "system",
        ai.MessageRole.User => "user",
        ai.MessageRole.Assistant => "assistant",
        ai.MessageRole.Tool => "tool",
    }
}

function _client_prior_turns(state: ClientConversation) -> string throws never {
    if (state.history.entries.length() <= state.initial_history_len) {
        return "";
    }
    state
        .history
        .entries
        .slice(state.initial_history_len, state.history.entries.length())
        .map((message: ai.Message) -> string {
            _client_message_role(message.role()) + ": " + _client_message_text(message)
        })
        .join("\n")
}

function _client_turn_prompt<T>(
    state: ClientConversation,
    output_suffix: string,
) -> baml.llm.PromptAst throws ai.InvalidRequest {
    let rendered = state
        .recipe
        .render_with_output_format(state.owner, reflect.type_of<T>(), output_suffix);
    let prior = _client_prior_turns(state);
    if (prior.length() == 0) {
        rendered
    } else {
        baml.llm
            .assemble_prompt_ast(
                ["", "\n\nConversation so far:\n", ""],
                [ai.messages._prompt_text_content(rendered), prior],
            )
    }
}

function _client_execute<T>(
    provider: ClientProvider,
    state: ClientConversation,
    prompt: baml.llm.PromptAst,
) -> T throws baml.errors.UnknownError {
    let context = baml.llm.ExecutionContext {
        jinja_string: "",
        args: {},
        function_name: state.identity?.name ?? "anonymous",
        prompt_closure: (ctx: baml.llm.Context) -> baml.llm.PromptAst {
            prompt
        },
    };
    //# One reliability owner (D5): the ai path skips the legacy client
    //# retry loop — it has no effects gate and would stack with ai.retry.
    //# Reliability for tasks is expressed with ai.retry / ai.fallback on
    //# the provider.
    match (provider.client.retry) {
        let legacy: baml.llm.RetryPolicy => {
            log.warn(
                "client declares a legacy retry_policy; the ai execution path ignores it — wrap the task's provider in ai.retry instead",
            );
        },
        null => {},
    }
    provider.client.execute_once_oneshot<T>(context, 0) catch_all (e) {
        _ => throw baml.errors.UnknownError.with_message<never>(
            e,
            "baml.llm.Client provider step failed",
        )
    }
}

function _client_metadata(provider: ClientProvider) -> ai.ResponseMetadata throws never {
    let primitive = _render_primitive(provider);
    ai.ResponseMetadata {
        provider: provider.name(),
        model: primitive.options.model,
        request_id: null,
        finish_reason: "stop",
        usage: null,
        attributes: { "adapter": "baml.llm.Client" },
        raw: null,
    }
}

function _client_value_text<T>(value: T) -> string throws never {
    baml.json.to_string(value) catch (e) {
        _ => string.from(value)
    }
}

function _client_require_state<T>(
    provider: ClientProvider,
    conversation: ai.Conversation,
) -> ClientConversation throws ai.InvalidRequest {
    ai.internal._require_conversation_output_type<T>(conversation, provider.name());
    match (conversation) {
        let state: ClientConversation => {
            if (!ai.internal._same_provider_instance(state.owner, provider)) {
                throw ai.InvalidRequest {
                    provider: provider.name(),
                    status_code: null,
                    detail: "client adapter received a conversation owned by another client",
                }
            }
            state
        },
        _ => throw ai.InvalidRequest {
            provider: provider.name(),
            status_code: null,
            detail: "client adapter received another provider's conversation",
        },
    }
}

function _client_validate_calls(
    provider: ClientProvider,
    calls: ai.tools.ToolCalls,
) -> null throws ai.InvalidRequest {
    let ids: string[] = [];
    for (let call in calls.calls) {
        if (call.id.length() == 0 || ids.includes(call.id)) {
            throw ai.InvalidRequest {
                provider: provider.name(),
                status_code: null,
                detail: "client adapter tool calls must have unique, non-empty IDs",
            }
        }
        ids.push(call.id);
    }
    null
}

implements ai.AgentProvider for ClientProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
        let history = task.messages().to_history();
        ClientConversation {
            owner: self,
            history: history,
            initial_history_len: history.entries.length(),
            declared_output_type: ai.internal._output_type_fingerprint<T>(),
            recipe: task._render,
            identity: task.identity,
            arguments: task.arguments,
            tags: task.tags,
            pending: [],
        }
    }

    function step<T>(
        self,
        conversation: ai.Conversation,
        tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
        let state = _client_require_state<T>(self, conversation);
        if (state.pending.length() != 0) {
            throw ai.InvalidRequest {
                provider: self.name(),
                status_code: null,
                detail: "submit tool results before requesting the next client adapter step",
            }
        }

        if (tools.length() == 0) {
            let value = _client_execute<T>(self, state, _client_turn_prompt<T>(state, ""));
            state.history.entries.push(ai.ChatMessage.assistant(_client_value_text(value)));
            return ai.ModelStep<T> {
                assistant_text: null,
                reasoning_text: null,
                outcome: value,
                metadata: _client_metadata(self),
            }
        }

        let choice = _client_execute<T | ai.tools.ToolCalls>(
            self,
            state,
            _client_turn_prompt<T | ai.tools.ToolCalls>(
                state,
                _client_tool_output_suffix(tools),
            ),
        );
        match (choice) {
            let calls: ai.tools.ToolCalls => {
                _client_validate_calls(self, calls);
                state.pending = calls.calls;
                state.history.entries.push(
                    ai
                        .ChatMessage
                        .assistant(
                            baml.json.stringify(
                                {
                                    "calls": calls.calls.map((call: ai.tools.ToolCall) -> json {
                                        {
                                            "id": call.id,
                                            "name": call.name,
                                            "args": call.args,
                                        }
                                    }),
                                },
                            ),
                        ),
                );
                ai.ModelStep<T> {
                    assistant_text: null,
                    reasoning_text: null,
                    outcome: calls,
                    metadata: _client_metadata(self),
                }
            },
            let value: T => {
                state.history.entries.push(ai.ChatMessage.assistant(_client_value_text(value)));
                ai.ModelStep<T> {
                    assistant_text: null,
                    reasoning_text: null,
                    outcome: value,
                    metadata: _client_metadata(self),
                }
            },
        }
    }

    function submit(
        self,
        conversation: ai.Conversation,
        results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws ai.Failure {
        let state = match (conversation) {
            let value: ClientConversation => value,
            _ => throw ai.InvalidRequest {
                provider: self.name(),
                status_code: null,
                detail: "client adapter received another provider's conversation",
            },
        };
        if (!ai.internal._same_provider_instance(state.owner, self)) {
            throw ai.InvalidRequest {
                provider: self.name(),
                status_code: null,
                detail: "client adapter received a conversation owned by another client",
            }
        }
        if (results.length() != state.pending.length()) {
            throw ai.InvalidRequest {
                provider: self.name(),
                status_code: null,
                detail: "client adapter requires exactly one result per pending tool call",
            }
        }
        let ordered: ai.tools.ToolResult[] = [];
        let seen: string[] = [];
        for (let call in state.pending) {
            let matches = results.filter((result: ai.tools.ToolResult) -> bool {
                ai.tools.result_id(result) == call.id
            });
            if (matches.length() != 1 || seen.includes(call.id)) {
                throw ai.InvalidRequest {
                    provider: self.name(),
                    status_code: null,
                    detail: "client adapter tool results must correlate exactly once",
                }
            }
            ordered.push(matches[0]);
            seen.push(call.id);
        }
        let content = "Tool results:\n"
            + baml.json.stringify(
                ordered.map((result: ai.tools.ToolResult) -> json {
                    {
                        "id": ai.tools.result_id(result),
                        "output": ai.tools.result_payload(result),
                        "is_error": ai.tools.result_is_error(result),
                    }
                }),
            );
        state.history.entries.push(
            ai.ChatMessage {
                message_role: ai.MessageRole.Tool,
                content: [ai.messages.TextPart { value: content, metadata: {} }],
                metadata: null,
            },
        );
        state.pending = [];
        state
    }
}
