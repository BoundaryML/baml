function _prompt_tool_schema(tool: ai.tools.Tool) -> json throws never {
    { "name": tool.name, "description": tool.description, "parameters": tool.input_schema }
}

function _prompt_tool_output_suffix(tools: ai.tools.Tool[]) -> string throws never {
    let schemas = tools.map((tool: ai.tools.Tool) -> json {
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

function _prompt_turn_task<T>(
    provider: root.openai.OpenAIProvider,
    state: PromptConversation,
    output_suffix: string,
) -> ai.Task<T> throws ai.InvalidRequest {
    let prompt = state.recipe.render_with_output_format(provider, reflect.type_of<T>(), output_suffix);
    let recipe = ai.PromptRenderRecipe.of(
        (ctx: baml.llm.Context) -> baml.llm.PromptAst {
            prompt
        },
    );
    ai.Task<T> {
        provider: provider,
        prompt: prompt,
        identity: state.identity,
        arguments: state.arguments,
        tools: [],
        tags: state.tags,
        _render: recipe,
    }
}

function _prompt_turn<T>(
    provider: root.openai.OpenAIProvider,
    state: PromptConversation,
    output_suffix: string,
) -> ai.ResponseWithMetadata<T> throws ai.Failure | baml.errors.UnknownError {
    let request = if (state.input.length() == 0) {
        let built = _responses_built_request(provider, _prompt_turn_task<T>(provider, state, output_suffix), false);
        let body = baml.json.parse(built.body) catch (e) {
            _ => {
                throw baml.errors.UnknownError.with_message<never>(
                    e,
                    "OpenAI prompt-tool request body was not JSON",
                )
            },
        };
        state.input = baml.json.path_or<json[]>(body, ".input", []);
        state.request_url = built.url;
        state.headers = _headers_without_auth(built.headers);
        built
    } else {
        _authenticated_request(
            provider,
            "POST",
            state.request_url,
            state.headers,
            baml.json.stringify(_request_body(provider, state.input)),
        )
    };
    log.info(
        baml.json
            .stringify(
                {
                    "event": "llm.request",
                    "provider": "openai-responses",
                    "model": provider.model,
                    "request_bytes": request.body.length(),
                    "output_mode": provider.output_mode.to_string(),
                    "tool_mode": provider.tool_mode.to_string(),
                },
            ),
    );
    let response = baml.http.send(request) catch (e) {
        _ => {
            throw ai.NetworkFailure {
                provider: "openai-responses",
                detail: "baml.http.send failed before a Responses API prompt-tool response arrived",
            }
        },
    };
    let text = response.text() catch (e) {
        _ => {
            throw ai.NetworkFailure {
                provider: "openai-responses",
                detail: "the Responses API prompt-tool body could not be read completely",
            }
        },
    };
    if (!response.ok()) {
        throw ai.internal._classify_http(
            "openai-responses",
            response.status_code,
            text,
        );
    }
    let raw = baml.json.parse(text) catch (e) {
        _ => {
            throw ai.ParseFailed {
                provider: "openai-responses",
                raw_output: text,
            }
        },
    };
    let value = _parse_response_value<T>(raw, text, true);
    let content = _response_output_text(raw) ?? "";
    state.input.push({ "role": "assistant", "content": content });
    state.history.entries.push(ai.ChatMessage.assistant(content));
    ai.ResponseWithMetadata<T> {
        value: value,
        metadata: _response_metadata(raw, provider.model),
        conversation: state,
    }
}

function _prompt_step<T>(
    provider: root.openai.OpenAIProvider,
    conversation: ai.Conversation,
    tools: ai.tools.Tool[],
) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
        let value: PromptConversation => value,
        _ => {
            throw ai.InvalidRequest {
                provider: "openai-responses",
                status_code: null,
                detail: "OpenAI prompt tool mode received another conversation type",
            }
        },
    };
    if (!ai.internal._same_provider_instance(state.owner, provider)) {
        throw ai.InvalidRequest {
            provider: "openai-responses",
            status_code: null,
            detail: "OpenAI prompt tool mode received a conversation owned by another provider instance",
        };
    }
    if (state.pending_call_ids.length() != 0) {
        throw ai.InvalidRequest {
            provider: "openai-responses",
            status_code: null,
            detail: "submit tool results before requesting the next OpenAI prompt-tool step",
        };
    }
    if (tools.length() == 0) {
        let response = _prompt_turn<T>(provider, state, "");
        return ai.ModelStep<T> {
            outcome: response.value,
            metadata: response.metadata,
            assistant_text: _response_output_text(response.metadata.raw),
            reasoning_text: _response_reasoning_text(response.metadata.raw),
        };
    }
    let response = _prompt_turn<T | ai.tools.ToolCalls>(provider, state, _prompt_tool_output_suffix(tools));
    match (response.value) {
        let calls: ai.tools.ToolCalls => {
            state.pending_call_ids
                = calls.calls.map((call: ai.tools.ToolCall) -> string {
                    call.id
                });
            let unique: string[] = [];
            for (let id in state.pending_call_ids) {
                if (id.length() == 0 || unique.includes(id)) {
                    throw ai.InvalidRequest {
                        provider: "openai-responses",
                        status_code: null,
                        detail: "prompt tool calls must have unique, non-empty call IDs",
                    };
                }
                unique.push(id);
            }
            ai.ModelStep<T> {
                outcome: calls,
                metadata: response.metadata,
                assistant_text: _response_output_text(response.metadata.raw),
                reasoning_text: _response_reasoning_text(response.metadata.raw),
            }
        },
        let value: T => {
            ai.ModelStep<T> {
                outcome: value,
                metadata: response.metadata,
                assistant_text: _response_output_text(response.metadata.raw),
                reasoning_text: _response_reasoning_text(response.metadata.raw),
            }
        },
    }
}

function _prompt_submit(
    provider: root.openai.OpenAIProvider,
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
) -> ai.Conversation throws ai.Failure {
    let state = match (conversation) {
        let value: PromptConversation => value,
        _ => {
            throw ai.InvalidRequest {
                provider: "openai-responses",
                status_code: null,
                detail: "OpenAI prompt tool mode received another conversation type",
            }
        },
    };
    if (!ai.internal._same_provider_instance(state.owner, provider)) {
        throw ai.InvalidRequest {
            provider: "openai-responses",
            status_code: null,
            detail: "OpenAI prompt tool mode received a conversation owned by another provider instance",
        };
    }
    let ordered = _correlate_tool_results(state.pending_call_ids, results);
    let wire_results = ordered.map((result: ai.tools.ToolResult) -> json {
        { "id": ai.tools.result_id(result), "output": ai.tools.result_payload(result), "is_error": ai.tools.result_is_error(result) }
    });
    let content = "Tool results:\n" + baml.json.stringify(wire_results);
    state.input.push({ "role": "user", "content": content });
    state
        .history
        .entries
        .push(
            ai.ChatMessage {
                message_role: ai.MessageRole.Tool,
                content: [ai.messages.TextPart { value: content, metadata: {} }],
                metadata: null,
            },
        );
    state.pending_call_ids = [];
    state
}
