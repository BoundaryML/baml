function TimeoutSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Hello`
}

// Duration options preserve v0's four HTTP budgets without changing the wire body.
function duration_options() -> bool {
    let duration = baml.time.Duration.from_milliseconds(1500);
    let clients = [
        openai.ResponsesClient.new(
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        openai.ChatClient.new(
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        openai.GenericClient.new(
            model = "offline",
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        openai.AzureClient.new(
            model = "offline",
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        openai.OpenRouterClient.new(
            model = "offline",
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        openai.OllamaClient.new(
            model = "offline",
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        anthropic.Client.new(
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        google.GeminiClient.new(
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        google.VertexClient.new(
            connect_timeout = duration,
            request_timeout = duration,
            time_to_first_token_timeout = duration,
            idle_timeout = duration,
        ),
        aws.BedrockClient.new(
            model = "offline",
            region = "us-east-1",
            connect_timeout = duration,
            request_timeout = duration,
        ),
        typesafeai.Client.new(
            model = "offline",
            connect_timeout = duration,
            request_timeout = duration,
        ),
    ];
    assert.equal(clients.length(), 11);
    assert.equal(duration.to_milliseconds(), 1500n);
    true
}

test "duration_http_options" {
    assert.is_true(duration_options())
}

function responses_duration_request_timeout() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok("{}")],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ResponsesClient.new(
                api_key = "test-key",
                base_url = mock.base_url(),
                request_timeout_ms = 5000,
                request_timeout = baml.time.Duration.from_milliseconds(100),
                connect_timeout = baml.time.Duration.from_milliseconds(1000),
            );
            let spec = TimeoutSpec@spec();
            cl.invoke(
                ai.ModelTurnInput {
                    prompt: spec.prompt_template,
                    journal: ai.Journal.new(spec),
                    toolbox: ai.tools.Toolbox.new([]),
                    output_type: spec.output_type(),
                },
            ) catch_all (e) {
                baml.errors.Timeout => {
                    return "timeout";
                },
                _ => {
                    return "wrong error";
                },
            };
            "unexpected success"
        },
        response_delay_ms = 1000,
    )
}

test "duration_request_deadline_overrides_legacy_ms" {
    assert.equal(responses_duration_request_timeout(), "timeout")
}

function shared_chat_duration_deadlines() -> bool {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok("{}")],
        (mock: root.llm_mock.MockJsonServer) -> {
            let deadline = baml.time.Duration.from_milliseconds(100);
            let connect = baml.time.Duration.from_milliseconds(1000);
            let clients: ai.Client[] = [
                openai.ChatClient.new(
                    api_key = "test-key",
                    base_url = mock.base_url(),
                    request_timeout = deadline,
                    connect_timeout = connect,
                ),
                openai.GenericClient.new(
                    model = "offline",
                    base_url = mock.base_url(),
                    request_timeout = deadline,
                    connect_timeout = connect,
                ),
                openai.AzureClient.new(
                    model = "offline",
                    api_key = "test-key",
                    base_url = mock.base_url(),
                    request_timeout = deadline,
                    connect_timeout = connect,
                ),
                openai.OpenRouterClient.new(
                    model = "offline",
                    api_key = "test-key",
                    base_url = mock.base_url(),
                    request_timeout = deadline,
                    connect_timeout = connect,
                ),
                openai.OllamaClient.new(
                    model = "offline",
                    base_url = mock.base_url(),
                    request_timeout = deadline,
                    connect_timeout = connect,
                ),
            ];
            let spec = TimeoutSpec@spec();
            let input = ai.ModelTurnInput {
                prompt: spec.prompt_template,
                journal: ai.Journal.new(spec),
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            };
            for (let cl in clients) {
                let outcome = cl.invoke(input) catch_all (e) {
                    baml.errors.Timeout => {
                        continue;
                    },
                    _ => {
                        throw baml.panics.AssertionFailed { message: `expected deadline from ${cl.id()}: ${e.to_string()}` };
                    },
                };
                let _ = outcome;
                throw baml.panics.AssertionFailed { message: `expected deadline from ${cl.id()}` };
            }
            true
        },
        response_delay_ms = 1000,
    )
}

test "all_shared_chat_clients_forward_request_deadlines" {
    assert.is_true(shared_chat_duration_deadlines())
}
