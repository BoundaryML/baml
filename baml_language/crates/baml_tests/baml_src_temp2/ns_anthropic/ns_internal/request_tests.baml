class EmptyStrictFixture {
}

class AnthropicRequestFixture {
    answer: string,
}

function _anthropic_request_fixture_task(
    provider: root.ai.Provider,
) -> root.ai.Task<AnthropicRequestFixture> throws never {
    root.ai.task<AnthropicRequestFixture>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return the fixture.\n", ""], [ctx.output_format])
    })
}

function _anthropic_image_fixture_task(
    provider: root.ai.Provider,
) -> root.ai.Task<string> throws never {
    let value = image.from_base64("iVBORw0KGgo=", "image/png");
    root.ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(["Describe this image: ", "\n", ""], [value, ctx.output_format])
    })
}

function _anthropic_pending_conversation(
    provider: root.anthropic.Messages,
) -> root.ai.Conversation throws never {
    let pending: json[] = [
        { "type": "tool_use", "id": "first", "name": "first_tool", "input": {} },
        { "type": "tool_use", "id": "second", "name": "second_tool", "input": {} },
    ];
    Conversation {
        owner: provider,
        history: root.ai.MessageHistory.empty(),
        declared_output_type: reflect.type_of<AnthropicRequestFixture>().to_string(),
        system: [],
        messages: [
            {
                "role": "assistant",
                "content": [
                    { "type": "tool_use", "id": "first", "name": "first_tool", "input": {} },
                    { "type": "tool_use", "id": "second", "name": "second_tool", "input": {} },
                ],
            },
        ],
        request_url: "https://example.test/v1/messages",
        headers: { "content-type": "application/json" },
        active_tools: [],
        last_assistant_content: pending,
    }
}

function _anthropic_submit_result(
    provider: root.anthropic.Messages,
    results: root.ai.tools.ToolResult[],
) -> string throws never {
    provider.submit(_anthropic_pending_conversation(provider), results) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            return "invalid";
        },
        _ => {
            return "other";
        },
    };
    "accepted"
}

test "Anthropic strict request uses output_config and API-key auth" {
    let provider = root
        .anthropic
        .messages(
            model = "claude-test",
            api_key = "test-anthropic-key",
            base_url = "https://example.test",
            output_mode = root.anthropic.OutputMode.Strict,
        );
    let request = _messages_built_request(provider, _anthropic_request_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert.equal(request.url, "https://example.test/v1/messages");
    assert.equal(request.headers.get("x-api-key"), "test-anthropic-key");
    assert.equal(baml.json.path<string>(body, ".output_config.format.type"), "json_schema");
    assert.equal(baml.json.path<string>(body, ".output_config.format.schema.type"), "object")
}

test "Anthropic SAP request omits native output_config" {
    let provider = root
        .anthropic
        .messages(
            model = "claude-test",
            api_key = "test-anthropic-key",
            base_url = "https://example.test",
            output_mode = root.anthropic.OutputMode.Sap,
        );
    let request = _messages_built_request(provider, _anthropic_request_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path_or<json?>(body, ".output_config", null), null)
}

test "Anthropic strict schema accepts an empty class" {
    let schema = _output_schema(reflect.type_of<EmptyStrictFixture>());
    let properties = baml.json.path<map<string, json>>(schema, ".properties");
    assert.equal(properties.keys().length(), 0);
    assert.equal(baml.json.path<bool>(schema, ".additionalProperties"), false)
}

test "Anthropic request preserves structural image content" {
    let provider = root
        .anthropic
        .messages(
            model = "claude-test",
            api_key = "test-anthropic-key",
            base_url = "https://example.test",
            output_mode = root.anthropic.OutputMode.Sap,
        );
    let request = _messages_built_request(provider, _anthropic_image_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path<string>(body, ".messages[0].content[1].type"), "image");
    assert.equal(baml.json.path<string>(body, ".messages[0].content[1].source.type"), "base64");
    assert
        .equal(
            baml.json.path<string>(body, ".messages[0].content[1].source.media_type"),
            "image/png",
        )
}

test "Anthropic submit rejects duplicate results for parallel tool calls" {
    let provider = root.anthropic.messages(api_key = "test-anthropic-key", base_url = "https://example.test");
    assert
        .equal(
            _anthropic_submit_result(
                provider,
                [
                    root.ai.tools.ToolResult {
                        id: "first",
                        output: { "value": 1 },
                        is_error: false,
                    },
                    root.ai.tools.ToolResult {
                        id: "first",
                        output: { "value": 2 },
                        is_error: false,
                    },
                ],
            ),
            "invalid",
        )
}

test "Anthropic submit leaves conversation unchanged when a later result is invalid" {
    let provider = root.anthropic.messages(api_key = "test-anthropic-key", base_url = "https://example.test");
    let state = match (_anthropic_pending_conversation(provider)) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Anthropic conversation"),
    };
    let original_history_length = state.history.entries.length();
    let original_message_length = state.messages.length();
    let rejected = false;
    let _ = provider
        .submit(
            state,
            [
                root.ai.tools.ToolResult { id: "first", output: { "value": 1 }, is_error: false },
                root.ai.tools.ToolResult { id: "unknown", output: { "value": 2 }, is_error: false },
            ],
        ) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected Anthropic submit error"),
    };
    assert.is_true(rejected);
    assert.equal(state.history.entries.length(), original_history_length);
    assert.equal(state.messages.length(), original_message_length);
    assert.equal(state.last_assistant_content.length(), 2)
}

test "Anthropic submit restores provider tool order" {
    let provider = root.anthropic.messages(api_key = "test-anthropic-key", base_url = "https://example.test");
    let conversation = _anthropic_pending_conversation(provider);
    let submitted = provider
        .submit(
            conversation,
            [
                root.ai.tools.ToolResult { id: "second", output: { "value": 2 }, is_error: false },
                root.ai.tools.ToolResult { id: "first", output: { "value": 1 }, is_error: false },
            ],
        );
    match (submitted) {
        let state: Conversation => {
            assert
                .equal(
                    baml.json.path<string>(state.messages[1], ".content[0].tool_use_id"),
                    "first",
                );
            assert
                .equal(
                    baml.json.path<string>(state.messages[1], ".content[1].tool_use_id"),
                    "second",
                )
        },
        _ => assert.is_true(false),
    }
}

test "Anthropic prompt submit correlates every pending call exactly once" {
    let provider = root
        .anthropic
        .messages(
            api_key = "unused",
            base_url = "https://example.test",
            output_mode = root.anthropic.OutputMode.Sap,
            tool_mode = root.anthropic.ToolMode.Prompt,
        );
    let state = match (_prompt_begin<AnthropicRequestFixture>(provider, _anthropic_request_fixture_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected Anthropic prompt conversation"),
    };
    state.pending_calls
        = [
            root.ai.tools.ToolCall { id: "first", name: "lookup", args: {} },
            root.ai.tools.ToolCall { id: "second", name: "lookup", args: {} },
        ];
    let rejected = false;
    let _ = _prompt_submit(
        provider,
        state,
        [
            root.ai.tools.ToolResult { id: "first", output: "one", is_error: false },
            root.ai.tools.ToolResult { id: "first", output: "duplicate", is_error: false },
        ],
    ) catch (e) {
        let invalid: root.ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected Anthropic prompt submit error"),
    };
    assert.is_true(rejected)
}
