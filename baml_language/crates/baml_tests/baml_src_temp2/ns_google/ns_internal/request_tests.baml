class RequestFixture {
    answer: string,
}

function _request_fixture_task(
    provider: ai.Provider,
) -> ai.Task<RequestFixture> throws never {
    ai.task<RequestFixture>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(["Return the fixture.\n", ""], [ctx.output_format])
    })
}

function _reserved_tool_handler() -> string throws never {
    "reserved"
}

test "Google AI native request uses responseJsonSchema and API-key auth" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
            output_mode = root.google.ai.OutputMode.Native,
        );
    let request = root.google.ai.internal._build_request(provider, _request_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert.equal(request.url, "https://example.test/v1beta/models/gemini-test:generateContent");
    assert.equal(request.headers.get("x-goog-api-key"), "test-google-key");
    assert
        .equal(
            baml.json.path<string>(body, ".generationConfig.responseMimeType"),
            "application/json",
        );
    assert
        .equal(baml.json.path<string>(body, ".generationConfig.responseJsonSchema.type"), "object");
    assert.equal(baml.json.path_or<json?>(body, ".tools", null), null)
}

test "Google AI SAP request omits native structured-output fields" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
            output_mode = root.google.ai.OutputMode.Sap,
        );
    let request = root.google.ai.internal._build_request(provider, _request_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert.equal(baml.json.path_or<json?>(body, ".generationConfig.responseJsonSchema", null), null);
    assert.equal(baml.json.path_or<json?>(body, ".generationConfig.responseMimeType", null), null)
}

test "Google provider extras cannot replace Task contents or owned protocol fields" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
            output_mode = root.google.ai.OutputMode.Native,
            extra_body = {
                "contents": "must-not-win",
                "systemInstruction": "must-not-win",
                "tools": "must-not-win",
                "toolConfig": "must-not-win",
                "generationConfig": {
                    "temperature": 0.25,
                    "responseMimeType": "text/plain",
                    "responseJsonSchema": { "type": "string" },
                },
            },
        );
    let request = root.google.ai.internal._build_request(provider, _request_fixture_task(provider));
    let body = baml.json.parse(request.body);
    assert
        .contains(baml.json.path<string>(body, ".contents[0].parts[0].text"), "Return the fixture");
    assert.equal(baml.json.path_or<json?>(body, ".systemInstruction", null), null);
    assert.equal(baml.json.path_or<json?>(body, ".tools", null), null);
    assert.equal(baml.json.path_or<json?>(body, ".toolConfig", null), null);
    // Non-owned generation tuning survives while the adapter's output
    // protocol wins over conflicting extra_body values.
    assert.equal(baml.json.path<float>(body, ".generationConfig.temperature"), 0.25);
    assert
        .equal(
            baml.json.path<string>(body, ".generationConfig.responseMimeType"),
            "application/json",
        );
    assert
        .equal(baml.json.path<string>(body, ".generationConfig.responseJsonSchema.type"), "object");

    let vertex = root
        .google
        .vertex
        .gemini(
            model = "gemini-test",
            api_key = "test-express-key",
            extra_body = { "contents": "must-not-win", "systemInstruction": "must-not-win" },
        );
    let vertex_request = root.google.vertex.internal._build_request(vertex, _request_fixture_task(vertex));
    let vertex_body = baml.json.parse(vertex_request.body);
    assert
        .contains(
            baml.json.path<string>(vertex_body, ".contents[0].parts[0].text"),
            "Return the fixture",
        );
    assert.equal(baml.json.path_or<json?>(vertex_body, ".systemInstruction", null), null)
}

test "Vertex API key uses the project-less Express Mode endpoint" {
    let provider = root
        .google
        .vertex
        .gemini(
            model = "gemini-test",
            api_key = "test-express-key",
            output_mode = root.google.vertex.OutputMode.Native,
        );
    let request = root.google.vertex.internal._build_request(provider, _request_fixture_task(provider));
    assert
        .equal(
            request.url,
            "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-test:generateContent?key=test-express-key",
        );
    assert.equal(request.headers.get("authorization"), null)
}

test "Google conversation tokens preserve wire state without sealing credentials" {
    let provider = root.google.vertex.gemini(model = "gemini-test", api_key = "credential-must-not-be-sealed");
    let conversation = provider.begin<RequestFixture>(_request_fixture_task(provider));
    let token = provider.save_conversation(conversation);
    assert.equal(token.sealed.includes("credential-must-not-be-sealed"), false);
    let restored = provider.restore_conversation(token);
    match (restored) {
        let state: Conversation => {
            assert.equal(state.provider_name, "google-vertex");
            assert.equal(state.contents.length(), 1);
            assert
                .equal(
                    state.url,
                    "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-test:generateContent?key=credential-must-not-be-sealed",
                )
        },
        _ => assert.is_true(false),
    }
}

test "Google message import rebuilds Gemini user and model contents" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
        );
    let messages = ai.MessageHistory {
        entries: [ai.ChatMessage.user("hello"), ai.ChatMessage.assistant("hi")],
    };
    let imported = provider.import_messages<RequestFixture>(messages);
    assert.equal(imported.fidelity, ai.ConversationFidelity.MessagesOnly);
    match (imported.conversation) {
        let state: Conversation => {
            assert.equal(state.contents.length(), 2);
            assert.equal(baml.json.path<string>(state.contents[0], ".role"), "user");
            assert.equal(baml.json.path<string>(state.contents[1], ".role"), "model")
        },
        _ => assert.is_true(false),
    }
}

test "Google submit rejects duplicate or missing result ids" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
        );
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Google conversation"),
    };
    state.last_calls
        = [
            { "id": "id1", "name": "first", "args": {} },
            { "id": "id2", "name": "second", "args": {} },
        ];
    let duplicate_rejected = false;
    let _duplicate = _submit(
        state,
        [
            ai.tools.ToolResult { id: "id1", output: { "value": "one" }, is_error: false },
            ai.tools.ToolResult {
                id: "id1",
                output: { "value": "duplicate" },
                is_error: false,
            },
        ],
        provider.name(),
    ) catch (e) {
        let invalid: ai.InvalidRequest => {
            duplicate_rejected = true;
            state
        },
        _ => state,
    };
    assert.is_true(duplicate_rejected);

    let missing_rejected = false;
    let _missing = _submit(
        state,
        [ai.tools.ToolResult { id: "id1", output: { "value": "one" }, is_error: false }],
        provider.name(),
    ) catch (e) {
        let invalid: ai.InvalidRequest => {
            missing_rejected = true;
            state
        },
        _ => state,
    };
    assert.is_true(missing_rejected)
}

test "Google submit emits function responses in provider call order" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "test-google-key",
            base_url = "https://example.test/v1beta",
        );
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Google conversation"),
    };
    state.last_calls
        = [
            { "id": "id1", "name": "first", "args": {} },
            { "id": "id2", "name": "second", "args": {} },
        ];
    _submit(
        state,
        [
            ai.tools.ToolResult { id: "id2", output: { "value": "two" }, is_error: false },
            ai.tools.ToolResult { id: "id1", output: { "value": "one" }, is_error: false },
        ],
        provider.name(),
    );
    let response_content = state.contents[state.contents.length() - 1];
    assert.equal(baml.json.path<string>(response_content, ".parts[0].functionResponse.id"), "id1");
    assert.equal(baml.json.path<string>(response_content, ".parts[1].functionResponse.id"), "id2")
}

test "Google reserves its typed-output tool name in SAP and Native modes" {
    let reserved = ai.tools.tool(_reserved_tool_handler, name = "__baml_return_output");
    let rejected = false;
    let _ = _validate_application_tool_names([reserved], "google-ai") catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            null
        },
    };
    assert.is_true(rejected)
}

test "Google prompt mode requires pending results before another step" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "unused",
            base_url = "https://example.test/v1beta",
            output_mode = root.google.ai.OutputMode.Sap,
            tool_mode = root.google.ai.ToolMode.Prompt,
        );
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected Google prompt conversation"),
    };
    state.last_calls = [ai.tools.ToolCall { id: "pending", name: "lookup", args: {} }];
    let rejected = false;
    let _ = provider.step<RequestFixture>(state, []) catch (e) {
        let invalid: ai.InvalidRequest => {
            rejected = true;
            ai.ModelStep<RequestFixture> {
                outcome: RequestFixture { answer: "unreachable" },
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
        _ => baml.sys.panic("unexpected Google prompt error"),
    };
    assert.is_true(rejected)
}

test "Google AI appends a fresh user message without losing thought signatures" {
    let provider = root
        .google
        .ai
        .gemini(model = "gemini-test", api_key = "unused", base_url = "https://example.test/v1beta");
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Google AI conversation"),
    };
    let output_type = state.declared_output_type;
    state
        .contents
        .push(
            {
                "role": "model",
                "parts": [{ "text": "first answer", "thoughtSignature": "opaque-signature" }],
            },
        );
    let original_wire_length = state.contents.length();
    let appended = provider.append_message(state, ai.ChatMessage.user("second turn"));
    match (appended) {
        let exact: Conversation => {
            assert.equal(exact.declared_output_type, output_type);
            assert.equal(exact.contents.length(), original_wire_length + 1);
            assert
                .equal(
                    baml.json
                        .path<string>(
                            exact.contents[original_wire_length - 1],
                            ".parts[0].thoughtSignature",
                        ),
                    "opaque-signature",
                );
            assert
                .equal(
                    baml.json.path<string>(exact.contents[original_wire_length], ".parts[0].text"),
                    "second turn",
                );
            assert.equal(ai.message_text(exact.history.entries[1]), "second turn")
        },
        _ => assert.is_true(false),
    }
}

test "Google Vertex append retains backend ownership and output type" {
    let provider = root.google.vertex.gemini(model = "gemini-test", api_key = "unused");
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Vertex conversation"),
    };
    let output_type = state.declared_output_type;
    let appended = provider.append_message(state, ai.ChatMessage.user("vertex follow up"));
    match (appended) {
        let exact: Conversation => {
            assert.equal(exact.provider_name, "google-vertex");
            assert.equal(exact.declared_output_type, output_type);
            assert.equal(ai.message_text(exact.history.entries[1]), "vertex follow up")
        },
        _ => assert.is_true(false),
    }
}

test "Google rejects append from another instance and while calls are pending" {
    let owner = root
        .google
        .ai
        .gemini(model = "gemini-test", api_key = "unused", base_url = "https://example.test/v1beta");
    let other = root
        .google
        .ai
        .gemini(model = "gemini-test", api_key = "unused", base_url = "https://example.test/v1beta");
    let state = match (owner.begin<RequestFixture>(_request_fixture_task(owner))) {
        let value: Conversation => value,
        _ => baml.sys.panic("expected Google conversation"),
    };
    let wrong_owner_rejected = false;
    let _wrong = other.append_message(state, ai.ChatMessage.user("wrong owner")) catch (e) {
        let invalid: ai.InvalidRequest => {
            wrong_owner_rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected Google append error"),
    };
    assert.is_true(wrong_owner_rejected);

    state.last_calls = [{ "id": "pending", "name": "lookup", "args": {} }];
    let pending_rejected = false;
    let _pending = owner.append_message(state, ai.ChatMessage.user("too soon")) catch (e) {
        let invalid: ai.InvalidRequest => {
            pending_rejected = true;
            state
        },
        _ => baml.sys.panic("unexpected Google pending append error"),
    };
    assert.is_true(pending_rejected);
    assert.equal(state.history.entries.length(), 1)
}

test "Google prompt-tool conversations append locally after a completed turn" {
    let provider = root
        .google
        .ai
        .gemini(
            model = "gemini-test",
            api_key = "unused",
            base_url = "https://example.test/v1beta",
            output_mode = root.google.ai.OutputMode.Sap,
            tool_mode = root.google.ai.ToolMode.Prompt,
        );
    let state = match (provider.begin<RequestFixture>(_request_fixture_task(provider))) {
        let value: PromptConversation => value,
        _ => baml.sys.panic("expected Google prompt conversation"),
    };
    let appended = provider.append_message(state, ai.ChatMessage.user("follow up"));
    match (appended) {
        let exact: PromptConversation => {
            assert.equal(ai.message_text(exact.history.entries[1]), "follow up");
            assert.equal(exact.last_calls.length(), 0)
        },
        _ => assert.is_true(false),
    }
}
