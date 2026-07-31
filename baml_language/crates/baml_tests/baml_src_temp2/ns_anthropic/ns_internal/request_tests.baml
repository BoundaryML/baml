class EmptyStrictFixture {
}

class AnthropicRequestFixture {
    answer: string,
}

function _anthropic_request_fixture_task(
    provider: ai.Provider,
) -> ai.Task<AnthropicRequestFixture> throws ai.InvalidRequest {
    ai.task<AnthropicRequestFixture>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return the fixture.\n", ""], [ctx.output_format])
    })
}

function _anthropic_image_fixture_task(
    provider: ai.Provider,
) -> ai.Task<string> throws ai.InvalidRequest {
    let value = image.from_base64("iVBORw0KGgo=", "image/png");
    ai.task<string>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm
            .assemble_prompt_ast(["Describe this image: ", "\n", ""], [value, ctx.output_format])
    })
}

function _anthropic_pending_conversation(
    provider: root.anthropic.AnthropicProvider,
) -> ai.Conversation throws never {
    let pending: json[] = [
        { "type": "tool_use", "id": "first", "name": "first_tool", "input": {} },
        { "type": "tool_use", "id": "second", "name": "second_tool", "input": {} },
    ];
    Conversation {
        owner: provider,
        history: ai.MessageHistory.empty(),
        declared_output_type: ai.internal._output_type_fingerprint<AnthropicRequestFixture>(),
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
    provider: root.anthropic.AnthropicProvider,
    results: ai.tools.ToolResult[],
) -> string throws never {
    provider.submit(_anthropic_pending_conversation(provider), results) catch (e) {
        let invalid: ai.InvalidRequest => {
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
                    ai.tools.ToolOk {
                        id: "first",
                        output: { "value": 1 },
                    },
                    ai.tools.ToolOk {
                        id: "first",
                        output: { "value": 2 },
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
                ai.tools.ToolOk { id: "first", output: { "value": 1 } },
                ai.tools.ToolOk { id: "unknown", output: { "value": 2 } },
            ],
        ) catch (e) {
        let invalid: ai.InvalidRequest => {
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
                ai.tools.ToolOk { id: "second", output: { "value": 2 } },
                ai.tools.ToolOk { id: "first", output: { "value": 1 } },
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
            ai.tools.ToolCall { id: "first", name: "lookup", args: {} },
            ai.tools.ToolCall { id: "second", name: "lookup", args: {} },
        ];
    let rejected = false;
    let _ = _prompt_submit(
        provider,
        state,
        [
            ai.tools.ToolOk { id: "first", output: "one" },
            ai.tools.ToolOk { id: "first", output: "duplicate" },
        ],
    ) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected Anthropic prompt submit error"),
    };
    assert.is_true(rejected)
}

test "Anthropic appends a fresh user message without losing exact assistant blocks" {
    let provider = root
        .anthropic
        .messages(model = "claude-test", api_key = "unused", base_url = "https://example.test");
    let state = match (provider.begin<AnthropicRequestFixture>(_anthropic_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Anthropic conversation"),
    };
    let output_type = state.declared_output_type;
    let opaque: json = { "type": "thinking", "thinking": "sealed", "signature": "opaque-signature" };
    state.last_assistant_content = [opaque];
    state.messages.push({ "role": "assistant", "content": [opaque] });
    let original_wire_length = state.messages.length();
    let _ = provider.append_message(state, ai.ChatMessage.user("second turn"));
    assert.equal(state.declared_output_type, output_type);
    assert.equal(state.messages.length(), original_wire_length + 1);
    assert
        .equal(
            baml.json
                .path<string>(
                    state.messages[original_wire_length - 1],
                    ".content[0].signature",
                ),
            "opaque-signature",
        );
    assert
        .equal(
            baml.json.path<string>(state.messages[original_wire_length], ".content[0].text"),
            "second turn",
        );
    assert.equal(ai.message_text(state.history.entries[1]), "second turn")
}

test "Anthropic renders a steering append as a tagged user turn" {
    let provider = root
        .anthropic
        .messages(model = "claude-test", api_key = "unused", base_url = "https://example.test");
    let state = match (provider.begin<AnthropicRequestFixture>(_anthropic_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Anthropic conversation"),
    };
    let original_wire_length = state.messages.length();
    let _ = provider.append_message(state, ai.ChatMessage.steer("stay on budget"));
    assert
        .equal(
            baml.json.path<string>(state.messages[original_wire_length], ".role"),
            "user",
        );
    assert
        .equal(
            baml.json.path<string>(state.messages[original_wire_length], ".content[0].text"),
            "<steering>\nstay on budget\n</steering>",
        );
    //# The portable projection keeps the System role and steering metadata.
    let appended = state.history.entries[state.history.entries.length() - 1];
    assert.is_true(appended.role() == ai.MessageRole.System);
    assert
        .equal(
            baml.json.path_or(appended.provider_metadata(), ".steering", false),
            true,
        )
}

test "Anthropic appends media using the first-render image block shape" {
    let provider = root
        .anthropic
        .messages(model = "claude-test", api_key = "unused", base_url = "https://example.test");
    let state = match (provider.begin<AnthropicRequestFixture>(_anthropic_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Anthropic conversation"),
    };
    let message = ai.ChatMessage {
        message_role: ai.MessageRole.User,
        content: [
            ai.messages.TextPart { value: "compare with this", metadata: {} },
            ai.messages.ImagePart {
                value: image.from_base64("iVBORw0KGgo=", "image/png"),
                metadata: {},
            },
        ],
        metadata: null,
    };
    let _ = provider.append_message(state, message);
    let appended = state.messages[state.messages.length() - 1];
    assert.equal(baml.json.path<string>(appended, ".role"), "user");
    assert.equal(baml.json.path<string>(appended, ".content[0].type"), "text");
    assert.equal(baml.json.path<string>(appended, ".content[0].text"), "compare with this");
    assert.equal(baml.json.path<string>(appended, ".content[1].type"), "image");
    assert.equal(baml.json.path<string>(appended, ".content[1].source.type"), "base64");
    assert
        .equal(
            baml.json.path<string>(appended, ".content[1].source.media_type"),
            "image/png",
        );
    assert.equal(baml.json.path<string>(appended, ".content[1].source.data"), "iVBORw0KGgo=")
}

test "Anthropic append rejects audio and leaves the conversation unchanged" {
    let provider = root
        .anthropic
        .messages(model = "claude-test", api_key = "unused", base_url = "https://example.test");
    let state = match (provider.begin<AnthropicRequestFixture>(_anthropic_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Anthropic conversation"),
    };
    let original_wire_length = state.messages.length();
    let original_history_length = state.history.entries.length();
    let rejected = false;
    let _ = provider
        .append_messages(
            state,
            ai.MessageHistory {
                entries: [
                    ai.ChatMessage.user("first message commits only with the batch"),
                    ai.ChatMessage {
                        message_role: ai.MessageRole.User,
                        content: [
                            ai.messages.AudioPart {
                                value: audio.from_base64("AAAA", "audio/mp3"),
                                metadata: {},
                            },
                        ],
                        metadata: null,
                    },
                ],
            },
        ) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            null
        },
        _ => baml.sys.panic("unexpected Anthropic append error"),
    };
    assert.is_true(rejected);
    assert.equal(state.messages.length(), original_wire_length);
    assert.equal(state.history.entries.length(), original_history_length)
}

test "Anthropic response helpers split visible text and thinking text" {
    let raw = baml.json
        .parse(
            `{"content":[{"type":"thinking","thinking":"weighing options","signature":"sig"},{"type":"text","text":"partial answer"}]}`,
        );
    assert.equal(_response_thinking_text(raw) ?? "", "weighing options");
    assert.equal(_response_text(raw) ?? "", "partial answer")
}

test "Anthropic rejects append while native or prompt tool calls are pending" {
    let provider = root.anthropic.messages(api_key = "unused", base_url = "https://example.test");
    let native = _anthropic_pending_conversation(provider);
    let native_rejected = false;
    let _native = provider.append_message(native, ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: ai.InvalidRequest => {
            native_rejected = true;
            null
        },
        _ => baml.sys.panic("unexpected Anthropic append error"),
    };
    assert.is_true(native_rejected);

    let prompt_provider = root
        .anthropic
        .messages(
            api_key = "unused",
            output_mode = root.anthropic.OutputMode.Sap,
            tool_mode = root.anthropic.ToolMode.Prompt,
        );
    let prompt = match (
        prompt_provider
            .begin<AnthropicRequestFixture>(_anthropic_request_fixture_task(prompt_provider))
    ) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected Anthropic prompt conversation"),
    };
    prompt.pending_calls = [ai.tools.ToolCall { id: "pending", name: "lookup", args: {} }];
    let prompt_rejected = false;
    let _prompt = prompt_provider.append_message(prompt, ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: ai.InvalidRequest => {
            prompt_rejected = true;
            null
        },
        _ => baml.sys.panic("unexpected Anthropic prompt append error"),
    };
    assert.is_true(prompt_rejected)
}
