class OpenAIEmptyStrictFixture {
}

function _openai_state_machine_task(
    provider: ai.Provider,
) -> ai.Task<string> throws ai.InvalidRequest {
    ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return ok.\n", ""], [ctx.output_format])
    })
}

test "OpenAI strict schema accepts an empty class" {
    let tool = _output_tool(reflect.type_of<OpenAIEmptyStrictFixture>(), true);
    let value_schema = baml.json.path<json>(tool, ".parameters.properties.value");
    let properties = baml.json.path<map<string, json>>(value_schema, ".properties");
    assert.equal(properties.keys().length(), 0);
    assert.equal(baml.json.path<bool>(value_schema, ".additionalProperties"), false)
}

test "OpenAI tool results are correlated and ordered by pending call ID" {
    let ordered = _correlate_tool_results(
        ["call-a", "call-b"],
        [
            ai.tools.ToolOk { id: "call-b", output: { "value": "b" } },
            ai.tools.ToolOk { id: "call-a", output: { "value": "a" } },
        ],
    );
    assert.equal(ai.tools.result_id(ordered[0]), "call-a");
    assert.equal(ai.tools.result_id(ordered[1]), "call-b")
}

test "OpenAI prompt mode requires pending results before another step" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt conversation"),
    };
    state.pending_call_ids = ["call-a"];
    let rejected = false;
    let _ = _prompt_step<string>(provider, state, []) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            ai.ModelStep<string> {
                outcome: "unreachable",
                metadata: ai.ResponseMetadata {
                    provider: "test",
                    model: null,
                    request_id: null,
                    finish_reason: null,
                    usage: null,
                    attributes: {},
                    raw: null,
                },
            }
        },
        _ => baml.sys.panic("unexpected OpenAI prompt error"),
    };
    assert.is_true(rejected)
}

test "OpenAI prompt submit rejects duplicate result IDs" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt conversation"),
    };
    state.pending_call_ids = ["call-a", "call-b"];
    let rejected = false;
    let _ = _prompt_submit(
        provider,
        state,
        [
            ai.tools.ToolOk { id: "call-a", output: "a" },
            ai.tools.ToolOk { id: "call-a", output: "duplicate" },
        ],
    ) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected OpenAI prompt submit error"),
    };
    assert.is_true(rejected)
}

test "OpenAI appends a fresh user message without losing exact response state" {
    let provider = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected OpenAI conversation"),
    };
    let output_type = state.declared_output_type;
    state.previous_response_id = "resp_exact_123";
    state.input
        = [
            {
                "type": "function_call_output",
                "call_id": "reserved_result_call",
                "output": `{"value":"first answer"}`,
            },
        ];
    state.history.entries.push(ai.ChatMessage.assistant("first answer"));
    let _ = provider.append_message(state, ai.ChatMessage.user("second turn"));
    assert.equal(state.previous_response_id, "resp_exact_123");
    assert.equal(state.declared_output_type, output_type);
    assert.equal(state.input.length(), 2);
    let wire = baml.json.stringify(state.input);
    assert.contains(wire, "reserved_result_call");
    assert.contains(wire, "second turn");
    assert.equal(ai.message_text(state.history.entries[2]), "second turn")
}

test "OpenAI rejects append from another provider instance and while tools are pending" {
    let owner = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let other = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let state = match (owner.begin<string>(_openai_state_machine_task(owner))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected OpenAI conversation"),
    };
    let wrong_owner_rejected = false;
    let _wrong = other.append_message(state, ai.ChatMessage.user("wrong owner")) catch (e) {
        let invalid: ai.InvalidRequest => {
            wrong_owner_rejected = true;
            null
        },
        _ => baml.sys.panic("unexpected OpenAI append error"),
    };
    assert.is_true(wrong_owner_rejected);
    assert.equal(state.history.entries.length(), 1);

    state.last_output = [{ "type": "function_call", "call_id": "pending" }];
    let pending_rejected = false;
    let _pending = owner.append_message(state, ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: ai.InvalidRequest => {
            pending_rejected = true;
            null
        },
        _ => baml.sys.panic("unexpected OpenAI pending append error"),
    };
    assert.is_true(pending_rejected);
    assert.equal(state.history.entries.length(), 1)
}

test "OpenAI appends after submitted tool results without replaying them" {
    let provider = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected OpenAI conversation"),
    };
    state.previous_response_id = "resp_with_tool_call";
    state.input
        = [
            {
                "type": "function_call_output",
                "call_id": "resolved_application_call",
                "output": `{"result":"already committed"}`,
            },
        ];
    state
        .history
        .entries
        .push(
            ai.ChatMessage {
                message_role: ai.MessageRole.Tool,
                content: [ai.messages.TextPart { value: "already committed", metadata: {} }],
                metadata: null,
            },
        );
    let _ = state.append_message(ai.ChatMessage.user("continue after cancellation"));
    assert.equal(state.previous_response_id, "resp_with_tool_call");
    assert.equal(state.input.length(), 2);
    let wire = baml.json.stringify(state.input);
    assert.contains(wire, "resolved_application_call");
    assert.contains(wire, "continue after cancellation");
    assert.equal(state.history.entries.length(), 3)
}

test "OpenAI appends steering notes as developer items" {
    let provider = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected OpenAI conversation"),
    };
    let _ = provider.append_message(state, ai.ChatMessage.steer("answer in French"));
    let item = state.input[state.input.length() - 1];
    assert.equal(baml.json.path<string>(item, ".role"), "developer");
    assert.equal(baml.json.path<string>(item, ".content"), "answer in French");
    let entry = state.history.entries[state.history.entries.length() - 1];
    assert.is_true(entry.role() == ai.MessageRole.System)
}

test "OpenAI appends media user messages in content-array form" {
    let provider = root.openai.responses(api_key = "unused", base_url = "https://example.test/v1");
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected OpenAI conversation"),
    };
    let message = ai.ChatMessage {
        message_role: ai.MessageRole.User,
        content: [
            ai.messages.TextPart { value: "what is in this image?", metadata: {} },
            ai.messages.ImagePart {
                value: image.from_url("https://example.test/cat.png", null),
                metadata: {},
            },
        ],
        metadata: null,
    };
    let _ = provider.append_message(state, message);
    let item = state.input[state.input.length() - 1];
    assert.equal(baml.json.path<string>(item, ".role"), "user");
    let parts = baml.json.path<json[]>(item, ".content");
    assert.equal(parts.length(), 2);
    assert.equal(baml.json.path<string>(parts[0], ".type"), "input_text");
    assert.equal(baml.json.path<string>(parts[0], ".text"), "what is in this image?");
    assert.equal(baml.json.path<string>(parts[1], ".type"), "input_image");
    assert.equal(baml.json.path<string>(parts[1], ".image_url"), "https://example.test/cat.png")
}

test "OpenAI reads reasoning summaries from Responses output items" {
    let raw = baml.json.parse(`{
        "output": [
            {"type": "reasoning", "summary": [
                {"type": "summary_text", "text": "Weighed both options."},
                {"type": "summary_text", "text": "Chose the cheaper one."}
            ]},
            {"type": "message", "content": [{"type": "output_text", "text": "ok"}]}
        ]
    }`);
    assert.equal(
        _response_reasoning_text(raw) ?? "",
        "Weighed both options.\nChose the cheaper one.",
    );
    assert.equal(_response_output_text(raw) ?? "", "ok");
    let empty = baml.json.parse(`{"output": []}`);
    assert.is_true(_response_reasoning_text(empty) == null)
}

test "OpenAI prompt-tool conversations append locally after a completed turn" {
    let provider = root
        .openai
        .responses(
            api_key = "unused",
            output_mode = root.openai.OutputMode.Sap,
            tool_mode = root.openai.ToolMode.Prompt,
        );
    let state = match (provider.begin<string>(_openai_state_machine_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected OpenAI prompt conversation"),
    };
    state.input = [{ "role": "assistant", "content": "first answer" }];
    let _ = provider.append_message(state, ai.ChatMessage.user("follow up"));
    assert.equal(state.input.length(), 2);
    assert.equal(baml.json.path<string>(state.input[1], ".content"), "follow up");
    assert.equal(ai.message_text(state.history.entries[1]), "follow up")
}
