// Anthropic-private text-tool continuation. Prompt mode cannot delegate to a
// generic provider adapter: AgentProvider.step is the only normal model
// request boundary, and re-entering it would recurse. This state instead calls
// Anthropic's private one-turn request helper directly.
class PromptConversation {
    owner: root.anthropic.Messages,
    history: root.ai.MessageHistory,
    declared_output_type: string,
    recipe: root.ai.PromptRenderRecipe,
    identity: root.ai.TaskIdentity?,
    arguments: map<string, unknown>,
    tags: map<string, string>,
    pending_calls: root.ai.tools.ToolCall[],

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

function _prompt_turn<T>(
    state: PromptConversation,
    tools: root.ai.tools.Tool[],
    output_suffix: string,
) -> root.ai.Task<T> throws never {
    //# Render this turn with T or T | ToolCalls as the text output contract
    let rendered = state.recipe.render_with_output_format(state.owner, reflect.type_of<T>(), output_suffix);
    //# Reattach prior text tool calls and correlated results
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

function _prompt_begin<T>(
    provider: root.anthropic.Messages,
    task: root.ai.Task<T>,
) -> root.ai.Conversation throws never {
    PromptConversation {
        owner: provider,
        history: task.messages().to_history(),
        declared_output_type: root.ai.internal._output_type_fingerprint<T>(),
        recipe: task._render,
        identity: task.identity,
        arguments: task.arguments,
        tags: task.tags,
        pending_calls: [],
    }
}

function _prompt_step<T>(
    provider: root.anthropic.Messages,
    conversation: root.ai.Conversation,
    tools: root.ai.tools.Tool[],
) -> root.ai.ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
        let value: PromptConversation => value,
        _ => {
            throw root.ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "Anthropic prompt mode received another provider's conversation",
            }
        },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, provider)) {
        throw root.ai.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "Anthropic prompt mode received a conversation owned by another provider instance",
        };
    }
    if (state.pending_calls.length() != 0) {
        throw root.ai.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "submit tool results before requesting the next Anthropic prompt-tool step",
        };
    }
    //# A zero-tool Agent step asks for T directly in Strict or SAP form
    if (tools.length() == 0) {
        let response = _messages_generate<T>(provider, _prompt_turn<T>(state, [], ""));
        state
            .history
            .entries
            .push(root.ai.ChatMessage.assistant(_response_text(response.metadata.raw) ?? ""));
        return root.ai.ModelStep<T> { outcome: response.value, metadata: response.metadata };
    }
    //# Prompt mode asks for the text choice T | ToolCalls without native tools
    let response = _messages_generate<T | root.ai.tools.ToolCalls>(
            provider,
            _prompt_turn<T | root.ai.tools.ToolCalls>(
                    state,
                    tools,
                    _prompt_tool_output_suffix(tools),
                ),
        );
    match (response.value) {
        let calls: root.ai.tools.ToolCalls => {
            let ids: string[] = [];
            for (let call in calls.calls) {
                if (call.id.length() == 0 || ids.includes(call.id)) {
                    throw root.ai.InvalidRequest {
                        provider: "anthropic",
                        status_code: null,
                        detail: "Anthropic prompt tool calls must have unique, non-empty IDs",
                    };
                }
                ids.push(call.id);
            }
            state.pending_calls = calls.calls;
            //## Retain the selected calls for the next prompt turn
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
            state
                .history
                .entries
                .push(root.ai.ChatMessage.assistant(_response_text(response.metadata.raw) ?? ""));
            root.ai.ModelStep<T> { outcome: value, metadata: response.metadata }
        },
    }
}

function _prompt_submit(
    provider: root.anthropic.Messages,
    conversation: root.ai.Conversation,
    results: root.ai.tools.ToolResult[],
) -> root.ai.Conversation throws root.ai.Failure {
    let state = match (conversation) {
        let value: PromptConversation => value,
        _ => {
            throw root.ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "Anthropic prompt mode received another provider's conversation",
            }
        },
    };
    if (!root.ai.internal._same_provider_instance(state.owner, provider)) {
        throw root.ai.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "Anthropic prompt mode received a conversation owned by another provider instance",
        };
    }
    if (results.length() != state.pending_calls.length()) {
        throw root.ai.InvalidRequest {
            provider: "anthropic",
            status_code: null,
            detail: "tool results must match every pending Anthropic prompt-tool call",
        };
    }
    //# Validate the full result set, then append it in model call order.
    for (let call in state.pending_calls) {
        let pending = state.pending_calls.filter((candidate: root.ai.tools.ToolCall) -> bool {
            candidate.id == call.id
        });
        let matching = results.filter((candidate: root.ai.tools.ToolResult) -> bool {
            candidate.id == call.id
        });
        if (call.id.length() == 0 || pending.length() != 1 || matching.length() != 1) {
            throw root.ai.InvalidRequest {
                provider: "anthropic",
                status_code: null,
                detail: "tool result IDs must match pending Anthropic prompt-tool call IDs exactly once",
            };
        }
    }
    for (let call in state.pending_calls) {
        let result = results
            .filter((candidate: root.ai.tools.ToolResult) -> bool {
                candidate.id == call.id
            })[0];
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
    state.pending_calls = [];
    state
}
