class RequestFixture {
    answer: string,
}

function _request_fixture_task(
    provider: root.ai.Provider,
) -> root.ai.Task<RequestFixture> throws never {
    root.ai.task<RequestFixture>(provider, (ctx: baml.llm.Context) -> baml.llm.PromptAst {
        baml.llm.assemble_prompt_ast(
            ["Return the fixture.\n", ""],
            [ctx.output_format],
        )
    })
}

test "Google AI native request uses responseJsonSchema and API-key auth" {
    let provider = root.google.ai.gemini(
        model = "gemini-test",
        api_key = "test-google-key",
        base_url = "https://example.test/v1beta",
        output_mode = root.google.ai.OutputMode.Native,
    );
    let request = root.google.ai.internal._build_request(
        provider,
        _request_fixture_task(provider),
    );
    let body = baml.json.parse(request.body);
    assert.equal(
        request.url,
        "https://example.test/v1beta/models/gemini-test:generateContent",
    );
    assert.equal(request.headers.get("x-goog-api-key"), "test-google-key");
    assert.equal(
        baml.json.path<string>(body, ".generationConfig.responseMimeType"),
        "application/json",
    );
    assert.equal(
        baml.json.path<string>(body, ".generationConfig.responseJsonSchema.type"),
        "object",
    );
    assert.equal(baml.json.path_or<json?>(body, ".tools", null), null)
}

test "Google AI SAP request omits native structured-output fields" {
    let provider = root.google.ai.gemini(
        model = "gemini-test",
        api_key = "test-google-key",
        base_url = "https://example.test/v1beta",
        output_mode = root.google.ai.OutputMode.Sap,
    );
    let request = root.google.ai.internal._build_request(
        provider,
        _request_fixture_task(provider),
    );
    let body = baml.json.parse(request.body);
    assert.equal(
        baml.json.path_or<json?>(
            body,
            ".generationConfig.responseJsonSchema",
            null,
        ),
        null,
    );
    assert.equal(
        baml.json.path_or<json?>(
            body,
            ".generationConfig.responseMimeType",
            null,
        ),
        null,
    )
}

test "Vertex API key uses the project-less Express Mode endpoint" {
    let provider = root.google.vertex.gemini(
        model = "gemini-test",
        api_key = "test-express-key",
        output_mode = root.google.vertex.OutputMode.Native,
    );
    let request = root.google.vertex.internal._build_request(
        provider,
        _request_fixture_task(provider),
    );
    assert.equal(
        request.url,
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-test:generateContent?key=test-express-key",
    );
    assert.equal(request.headers.get("authorization"), null)
}

test "Google conversation tokens preserve wire state without sealing credentials" {
    let provider = root.google.vertex.gemini(
        model = "gemini-test",
        api_key = "credential-must-not-be-sealed",
    );
    let conversation = provider.begin<RequestFixture>(
        _request_fixture_task(provider),
    );
    let token = provider.save_conversation(conversation);
    assert.equal(token.sealed.includes("credential-must-not-be-sealed"), false);
    let restored = provider.restore_conversation(token);
    match (restored) {
        let state: Conversation => {
            assert.equal(state.provider_name, "google-vertex");
            assert.equal(state.contents.length(), 1);
            assert.equal(
                state.url,
                "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-test:generateContent?key=credential-must-not-be-sealed",
            )
        },
        _ => assert.is_true(false),
    }
}

test "Google message import rebuilds Gemini user and model contents" {
    let provider = root.google.ai.gemini(
        model = "gemini-test",
        api_key = "test-google-key",
        base_url = "https://example.test/v1beta",
    );
    let messages = root.ai.MessageHistory {
        entries: [
            root.ai.ChatMessage.user("hello"),
            root.ai.ChatMessage.assistant("hi"),
        ],
    };
    let imported = provider.import_messages(messages);
    assert.equal(imported.fidelity, root.ai.ConversationFidelity.MessagesOnly);
    match (imported.conversation) {
        let state: Conversation => {
            assert.equal(state.contents.length(), 2);
            assert.equal(
                baml.json.path<string>(state.contents[0], ".role"),
                "user",
            );
            assert.equal(
                baml.json.path<string>(state.contents[1], ".role"),
                "model",
            )
        },
        _ => assert.is_true(false),
    }
}
