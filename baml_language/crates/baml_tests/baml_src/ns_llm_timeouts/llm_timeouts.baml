// The `timeout` argument every LLM client takes: a `baml.time.Duration` (the
// end-to-end total) or an `ai.LlmTimeoutOptions` with all four deadlines.
//
// `timeout` and `connect_timeout` are the transport's (`baml.http`); the tests
// for those live with the transport (`crates/baml_tests/tests/http.rs`) and
// with each provider's corpus. What is asserted here is the part the LLM layer
// owns: `first_token_timeout` and `stream_token_timeout` count non-empty
// CONTENT deltas, not SSE events, and only the time spent waiting on the
// provider.
//
// Timing rule for this file: the corpus runs every test concurrently, so a
// deadline that must fire is paired with a mock delay far past it, and a
// deadline that must NOT fire is far past anything the mock does.

function TimeoutEcho(question: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `
        ${role("user")}
        ${question}
    `
}

function _timeout_input() -> ai.ModelTurnInput {
    let spec = TimeoutEcho@spec("hi");
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function _ms(milliseconds: int) -> baml.time.Duration {
    baml.time.Duration.from_milliseconds(milliseconds)
}

function _chat_json(text: string) -> string {
    `{"id":"c","object":"chat.completion","created":1,"model":"gpt-4o-mini","choices":[{"index":0,"message":{"role":"assistant","content":"${text}"},"finish_reason":"stop"}]}`
}

function _chat_delta(text: string) -> string {
    root.llm_mock.sse_data(`{"choices":[{"index":0,"delta":{"content":"${text}"}}]}`)
}

/// The label a timeout test compares: which deadline fired and its limit.
function _timeout_label(timeout: baml.errors.Timeout) -> string {
    `Timeout:${timeout.timeout_type}:${timeout.duration?.to_milliseconds() ?? -1n}`
}

/// Drains a stream to its final text, or the label of what stopped it.
function _drain(stream: ai.stream.TurnStream) -> string {
    `${stream.final_turn().terminal_text() ?? ""}` catch_all (e) {
        let timeout: baml.errors.Timeout => _timeout_label(timeout),
        _ => `other:${e.to_string()}`,
    }
}

// ─── the options type ────────────────────────────────────────────────────────

// A bare Duration is the total; null leaves every deadline at its default.
function llm_timeout_options_resolve() -> string {
    let from_duration = ai.LlmTimeoutOptions._resolve(_ms(1500));
    let from_null = ai.LlmTimeoutOptions._resolve(null);
    let given = ai.LlmTimeoutOptions { stream_token_timeout: _ms(7) };
    let from_options = ai.LlmTimeoutOptions._resolve(given);
    [
        `${from_duration.timeout?.to_milliseconds() ?? -1n}`,
        `${from_duration.connect_timeout == null}`,
        `${from_duration.first_token_timeout == null}`,
        `${from_null.timeout == null}`,
        `${from_options.stream_token_timeout?.to_milliseconds() ?? -1n}`,
        `${from_options.timeout == null}`,
    ]
        .join("|")
}

test "llm_timeout_options_resolve" {
    assert.equal(llm_timeout_options_resolve(), "1500|true|true|true|7|true")
}

// The two transport deadlines ride on the request itself.
function llm_timeout_options_apply() -> string {
    let options = ai.LlmTimeoutOptions { timeout: _ms(120000), connect_timeout: _ms(3000) };
    let request = options._apply(baml.http.Request.from_url("http://localhost").set_method("POST"));
    `${request.timeout?.to_milliseconds() ?? -1n}|${request.connect_timeout?.to_milliseconds() ?? -1n}`
}

test "llm_timeout_options_apply" {
    assert.equal(llm_timeout_options_apply(), "120000|3000")
}

// ─── first_token_timeout ─────────────────────────────────────────────────────

// Anthropic's `message_start` arrives at once and carries no content, so it
// does not end the first-token deadline: the content delta is held 10s and the
// 100ms deadline fires.
function first_token_timeout_ignores_message_start() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_event(
                "message_start",
                `{"type":"message_start","message":{"id":"m","model":"claude-haiku-4-5","usage":{"input_tokens":5}}}`,
            ),
            root.llm_mock.sse_event(
                "content_block_delta",
                `{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"pong"}}`,
            ),
            root.llm_mock.sse_event("message_stop", `{"type":"message_stop"}`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = anthropic.Client.new(
                model = "claude-haiku-4-5",
                api_key = "test-key",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(100) },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 10000,
    )
}

test "first_token_timeout_ignores_message_start" {
    assert.equal(first_token_timeout_ignores_message_start(), "Timeout:first_token_timeout:100")
}

// An empty text delta (the role-only first chunk of chat completions) is not
// content either.
function first_token_timeout_ignores_empty_delta() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}`,
            ),
            _chat_delta("pong"),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(100) },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 10000,
    )
}

test "first_token_timeout_ignores_empty_delta" {
    assert.equal(first_token_timeout_ignores_empty_delta(), "Timeout:first_token_timeout:100")
}

// Content that is not text ends it: a tool-input fragment arrives at once, so
// a deadline far shorter than the wait for the first text does not fire.
function first_token_timeout_ends_on_tool_input() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\\"q\\":"}}]}}]}`,
            ),
            _chat_delta("pong"),
            root.llm_mock.sse_data(`[DONE]`),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                // 5s to reach the tool input, which the mock sends immediately.
                // The text follows 6s later, after the deadline would have passed.
                timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(5000) },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 6000,
    )
}

test "first_token_timeout_ends_on_tool_input" {
    assert.equal(first_token_timeout_ends_on_tool_input(), "pong")
}

// ─── stream_token_timeout ────────────────────────────────────────────────────

// After the first content delta the gap to the next one is bounded. The total
// is far away, so only the 100ms stream-token deadline can end the call.
function stream_token_timeout_fires_between_deltas() -> string {
    root.llm_mock.mock_sse_serve(
        [_chat_delta("po"), _chat_delta("ng"), root.llm_mock.sse_data(`[DONE]`)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { timeout: _ms(60000), stream_token_timeout: _ms(100) },
            );
            let stream = cl.invoke_stream(_timeout_input());
            let first = match (stream.next()) {
                let batch: string[] => batch.join(""),
                let done: ai.stream.Done => "Done",
            };
            `${first}|${_drain(stream)}`
        },
        chunk_delay_ms = 10000,
    )
}

test "stream_token_timeout_fires_between_deltas" {
    assert.equal(stream_token_timeout_fires_between_deltas(), "po|Timeout:stream_token_timeout:100")
}

// It is not running before the first content delta: the first delta is held 3s
// against a 1.5s stream-token deadline, and the stream still completes.
function stream_token_timeout_waits_for_first_content() -> string {
    root.llm_mock.mock_sse_serve(
        [_chat_delta("po"), _chat_delta("ng"), root.llm_mock.sse_data(`[DONE]`)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { stream_token_timeout: _ms(1500) },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 1,
        first_event_delay_ms = 3000,
    )
}

test "stream_token_timeout_waits_for_first_content" {
    assert.equal(stream_token_timeout_waits_for_first_content(), "pong")
}

// A slow consumer is not charged: the caller sits on the first delta for far
// longer than the deadline while the rest of the stream arrives on time.
function stream_token_timeout_does_not_charge_slow_consumer() -> string {
    root.llm_mock.mock_sse_serve(
        [_chat_delta("po"), _chat_delta("ng"), root.llm_mock.sse_data(`[DONE]`)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { stream_token_timeout: _ms(1500) },
            );
            let stream = cl.invoke_stream(_timeout_input());
            let _ = stream.next();
            baml.sys.sleep(_ms(3000));
            _drain(stream)
        },
        chunk_delay_ms = 1,
    )
}

test "stream_token_timeout_does_not_charge_slow_consumer" {
    assert.equal(stream_token_timeout_does_not_charge_slow_consumer(), "pong")
}

// ─── zero or negative means no limit ─────────────────────────────────────────

function zero_token_timeouts_are_unbounded() -> string {
    root.llm_mock.mock_sse_serve(
        [_chat_delta("po"), _chat_delta("ng"), root.llm_mock.sse_data(`[DONE]`)],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(0), stream_token_timeout: _ms(-5) },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 50,
        first_event_delay_ms = 200,
    )
}

test "zero_token_timeouts_are_unbounded" {
    assert.equal(zero_token_timeouts_are_unbounded(), "pong")
}

// A zero total opts out of the 5 minute default instead of failing at once.
function zero_total_timeout_is_unbounded() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_json("pong"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(api_key = "k", base_url = mock.base_url(), timeout = _ms(0));
            cl.invoke(_timeout_input()).terminal_text() ?? ""
        },
        response_delay_ms = 100,
    )
}

test "zero_total_timeout_is_unbounded" {
    assert.equal(zero_total_timeout_is_unbounded(), "pong")
}

// ─── non-streaming calls ─────────────────────────────────────────────────────

// The two token deadlines belong to streams: a non-streaming call accepts them
// and ignores them, however short they are.
function invoke_ignores_token_timeouts() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_json("pong"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            let cl = openai.ChatClient.new(
                api_key = "k",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(1), stream_token_timeout: _ms(1) },
            );
            cl.invoke(_timeout_input()).terminal_text() ?? ""
        },
        response_delay_ms = 100,
    )
}

test "invoke_ignores_token_timeouts" {
    assert.equal(invoke_ignores_token_timeouts(), "pong")
}

// ─── clients that had no timeout before ──────────────────────────────────────

function _invoke_label(client: ai.Client) -> string {
    `turn:${client.invoke(_timeout_input()).terminal_text() ?? ""}` catch_all (e) {
        let timeout: baml.errors.Timeout => _timeout_label(timeout),
        _ => `other:${e.to_string()}`,
    }
}

function ollama_total_timeout() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_json("pong"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            _invoke_label(
                openai.OllamaClient.new(
                    model = "llama3.2",
                    base_url = mock.base_url(),
                    timeout = _ms(20),
                ),
            )
        },
        response_delay_ms = 2000,
    )
}

test "ollama_total_timeout" {
    assert.equal(ollama_total_timeout(), "Timeout:timeout:20")
}

function openrouter_total_timeout() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_json("pong"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            _invoke_label(
                openai.OpenRouterClient.new(
                    model = "openai/gpt-4o-mini",
                    api_key = "or-key",
                    base_url = mock.base_url(),
                    timeout = _ms(20),
                ),
            )
        },
        response_delay_ms = 2000,
    )
}

test "openrouter_total_timeout" {
    assert.equal(openrouter_total_timeout(), "Timeout:timeout:20")
}

function azure_total_timeout() -> string {
    root.llm_mock.mock_json_serve(
        [root.llm_mock.MockResponse.ok(_chat_json("pong"))],
        (mock: root.llm_mock.MockJsonServer) -> {
            _invoke_label(
                openai.AzureClient.new(
                    model = "gpt-4o",
                    api_key = "azure-key",
                    base_url = mock.base_url(),
                    timeout = _ms(20),
                ),
            )
        },
        response_delay_ms = 2000,
    )
}

test "azure_total_timeout" {
    assert.equal(azure_total_timeout(), "Timeout:timeout:20")
}

function gemini_total_timeout() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.ok(
                `{"candidates":[{"content":{"role":"model","parts":[{"text":"pong"}]},"finishReason":"STOP"}]}`,
            ),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            _invoke_label(
                google.GeminiClient.new(
                    model = "gemini-2.5-flash",
                    api_key = "test-key",
                    base_url = mock.base_url(),
                    timeout = _ms(20),
                ),
            )
        },
        response_delay_ms = 2000,
    )
}

test "gemini_total_timeout" {
    assert.equal(gemini_total_timeout(), "Timeout:timeout:20")
}

// Gemini streams: a thought part is content, so it ends the first-token
// deadline, and the stall after it is the stream-token deadline's.
function gemini_stream_token_timeout() -> string {
    root.llm_mock.mock_sse_serve(
        [
            root.llm_mock.sse_data(
                `{"candidates":[{"content":{"role":"model","parts":[{"text":"thinking","thought":true}]}}]}`,
            ),
            root.llm_mock.sse_data(
                `{"candidates":[{"content":{"role":"model","parts":[{"text":"pong"}]},"finishReason":"STOP"}]}`,
            ),
        ],
        (mock: root.llm_mock.MockSseServer) -> {
            let cl = google.GeminiClient.new(
                model = "gemini-2.5-flash",
                api_key = "test-key",
                base_url = mock.base_url(),
                timeout = ai.LlmTimeoutOptions {
                    first_token_timeout: _ms(5000),
                    stream_token_timeout: _ms(100),
                },
            );
            _drain(cl.invoke_stream(_timeout_input())) catch_all (e) {
                let timeout: baml.errors.Timeout => _timeout_label(timeout),
                _ => `other:${e.to_string()}`,
            }
        },
        chunk_delay_ms = 10000,
    )
}

test "gemini_stream_token_timeout" {
    assert.equal(gemini_stream_token_timeout(), "Timeout:stream_token_timeout:100")
}

// ─── a token timeout is a failed attempt ─────────────────────────────────────

// Fallback moves to the next member when the first one's first-token deadline
// fires, since nothing has been shown to the caller yet.
function fallback_advances_on_first_token_timeout() -> string {
    root.llm_mock.mock_sse_serve(
        [_chat_delta("slow"), root.llm_mock.sse_data(`[DONE]`)],
        (stalled: root.llm_mock.MockSseServer) -> {
            root.llm_mock.mock_sse_serve(
                [_chat_delta("po"), _chat_delta("ng"), root.llm_mock.sse_data(`[DONE]`)],
                (healthy: root.llm_mock.MockSseServer) -> {
                    let first = openai.ChatClient.new(
                        api_key = "k",
                        base_url = stalled.base_url(),
                        timeout = ai.LlmTimeoutOptions { first_token_timeout: _ms(100) },
                    );
                    let second = openai.ChatClient.new(api_key = "k", base_url = healthy.base_url());
                    let fallback = ai.clients.Fallback { members: [first, second] };
                    let text = _drain(fallback.invoke_stream(_timeout_input())) catch_all (e) {
                        _ => `other:${e.to_string()}`,
                    };
                    `${text}|${stalled.request_count()}|${healthy.request_count()}`
                },
                chunk_delay_ms = 1,
            )
        },
        first_event_delay_ms = 10000,
    )
}

test "fallback_advances_on_first_token_timeout" {
    assert.equal(fallback_advances_on_first_token_timeout(), "pong|1|1")
}
