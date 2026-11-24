class Answer {
    count: int,
}

function Extract() -> Answer {
    client: "openai/gpt-4o-mini"
    prompt: `Return a count. ${ctx.output_format()}`
}

function input() -> ai.ModelTurnInput {
    let spec = Extract@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal.new(spec),
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

function completed(text: string) -> string {
    baml.json.stringify(
        {
            "status": "completed",
            "output": [{ "type": "message", "content": [{ "type": "output_text", "text": text }] }],
        },
    )
}

function stream_schema_failure(drain_partials: bool) -> bool {
    let text = `{"count":"not a number"}`;
    root.llm_mock.mock_sse_serve([root.llm_mock.sse_data(baml.json.stringify({ "type": "response.output_text.delta", "delta": text })), root.llm_mock.sse_data(`{"type":"response.completed","response":{"status":"completed","output":[]}}`)], (
        mock: root.llm_mock.MockSseServer,
    ) -> {
        let cl = openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url());
        let stream = ai.stream.from_spec(Extract@spec(), client = cl);
        let result = {
            if (drain_partials) {
                while (!(stream.next() is ai.stream.Done)) {}
            }
            stream.final();
            false
        } catch_all (e) {
            let failed: ai.errors.ParseFailed => {
                failed.provider == "openai"
                    && failed.raw_output == text
                    && (failed.message ?? "") != ""
            },
            _ => false,
        };
        result
    })
}

test "stream final schema failure has AI classification and diagnostics" {
    assert.is_true(stream_schema_failure(false))
}

test "schema failure remains classified after partial consumption" {
    assert.is_true(stream_schema_failure(true))
}

test "direct SAP parsing retains its general parser error" {
    let result = baml.sap.parse<Answer>(`{"count":"not a number"}`) catch (e) {
        let parse: baml.errors.ParseError => {
            assert.is_true(parse.message != "");
            return;
        },
    };
    let _ = result;
    assert.is_true(false)
}

test "malformed HTTP 200 envelope retains body and actual status" {
    let raw = "not JSON";
    let preserved = root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(raw)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url());
        cl.invoke(input()) catch_all (e) {
            let invalid: ai.errors.InvalidResponse => {
                return invalid.raw_body == raw && invalid.status_code == 200 && invalid.message != "";
            },
            _ => {
                return false;
            },
        };
        false
    });
    assert.is_true(preserved)
}

function retry_reported_failure(code: string, expected_attempts: int) -> bool {
    let failure = baml.json.stringify(
        {
            "status": "failed",
            "error": { "code": code, "type": "provider_error", "message": "provider diagnostic" },
        },
    );
    root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(failure)], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );
        let preserved = {
            cl.invoke(input());
            false
        } catch_all (e) {
            let reported: ai.errors.ProviderError => {
                reported.error_code == code
                    && reported.error_type == "provider_error"
                    && reported.message == "provider diagnostic"
                    && reported.raw_body != null
                    && reported.status_code == null
                    && mock.request_count() == expected_attempts
            },
            _ => false,
        };
        preserved
    })
}

test "transient in-band provider failure retries and retains metadata" {
    assert.is_true(retry_reported_failure("server_error", 2))
}

test "permanent in-band provider failure is not retried" {
    assert.is_true(retry_reported_failure("invalid_prompt", 1))
}

test "normalization retains new error identities" {
    let invalid = ai.errors.InvalidResponse {
        provider: "test",
        message: "before",
        raw_body: "bad",
        status_code: 200,
    };
    let reported = ai.errors.ProviderError {
        provider: "test",
        message: "before",
        error_type: "overloaded",
        error_code: "server_error",
    };
    let normalized_invalid = ai.errors.normalize(invalid);
    let normalized_reported = ai.errors.normalize(reported);
    invalid.message = "after";
    reported.message = "after";
    match (normalized_invalid) {
        let same: ai.errors.InvalidResponse => assert.equal(same.message, "after"),
        _ => assert.is_true(false),
    };
    match (normalized_reported) {
        let same: ai.errors.ProviderError => assert.equal(same.message, "after"),
        _ => assert.is_true(false),
    }
}

// The EventStream transport turns :error-code/:error-message headers into a
// named event with plain-text data, not a modeled JSON exception payload.
test "Bedrock unmodeled error frame preserves plain text and native code" {
    let batch = baml.json.stringify([{ "event": "error:InternalFailure", "data": "stream failed" }]);
    aws.internal.decode_stream_batch(batch) catch_all (e) {
        let reported: ai.errors.ProviderError => {
            assert.equal(reported.message, "stream failed");
            assert.equal(reported.error_code, "InternalFailure");
            assert.equal(reported.error_type, "error:InternalFailure");
            assert.equal(reported.status_code, null);
            assert.is_true(ai.clients.default_retry_judgment(reported));
            return;
        },
        _ => assert.is_true(false),
    };
    assert.is_true(false)
}

function chat_reported_failure(streaming: bool) -> bool {
    let payload: json = { "error": { "message": "overloaded", "type": "server_error", "code": 503 } };
    let raw = baml.json.stringify(payload);
    let result = {
        if (streaming) {
            let decoder = openai.internal.ChatStreamDecoder.new("openai-chat");
            decoder.decode(baml.json.stringify([{ "data": raw }]));
        } else {
            let response = baml.json.to<openai.internal.CcResponse>(payload);
            openai.internal.chat_parse(response, "openai-chat", raw);
        }
        false
    } catch_all (e) {
        let reported: ai.errors.ProviderError => {
            reported.provider == "openai-chat"
                && reported.message == "overloaded"
                && reported.error_type == "server_error"
                && reported.error_code == "503"
                && reported.raw_body == raw
                && reported.status_code == null
        },
        _ => false,
    };
    result
}

test "Chat unary in-band failure keeps provider metadata" {
    assert.is_true(chat_reported_failure(false))
}

test "Chat stream in-band failure keeps provider metadata" {
    assert.is_true(chat_reported_failure(true))
}

test "retry call retains status-less provider failure before success" {
    let raw = `{"status":"failed","error":{"code":"server_error","message":"overloaded"}}`;
    let preserved = root.llm_mock.mock_json_serve([root.llm_mock.MockResponse.ok(raw), root.llm_mock.MockResponse.ok(completed("ok"))], (
        mock: root.llm_mock.MockJsonServer,
    ) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );
        let turn = cl.invoke(input());
        let failed = turn.child_turns.at(0);
        if let attempt: ai.ModelTurn = failed {
            assert.equal(attempt.stop_reason, ai.content.StopReason.Failed);
            // No response named a model.
            assert.equal(attempt.resolved_model, null);
            match (attempt.error) {
                let reported: ai.errors.ProviderError => {
                    return reported.message == "overloaded"
                        && reported.error_code == "server_error"
                        && reported.raw_body != null;
                },
                _ => {
                    return false;
                },
            };
        }
        false
    });
    assert.is_true(preserved)
}

// ─── Retry judgment for provider-rejected requests ──────────────────────────
//
// The default judgment retries an `InvalidRequest` that carries an HTTP status
// (the provider answered with an error status, which can be transient) and
// does not retry one raised without a status (BAML refused to build the
// request, so resending it cannot help). Parity with the legacy BAML DSL's
// retry policy, which repeated every request answered with an error status.

function rejected_body() -> string {
    `{"error":{"message":"bad request"}}`
}

function short_backoff() -> ai.clients.Backoff {
    ai.clients.Backoff.new(initial_ms = 1, max_ms = 1)
}

function rejection_label(e: unknown) -> string {
    match (e) {
        let invalid: ai.errors.InvalidRequest => `InvalidRequest/${invalid.status_code ?? -1}`,
        _ => `other:${e.to_string()}`,
    }
}

// Invokes a three-attempt Retry over a Responses client against `responses`;
// reports the outcome and how many requests reached the server.
function retry_over_http(
    responses: root.llm_mock.MockResponse[],
    retry_if: ((ai.errors.Failure) -> bool throws never)? = null,
) -> string {
    root.llm_mock.mock_json_serve(responses, (mock: root.llm_mock.MockJsonServer) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 3,
            retry_if = retry_if,
            backoff = short_backoff(),
        );
        let outcome = (cl.invoke(input()).terminal_text() ?? "-") catch_all (e) {
            _ => rejection_label(e),
        };
        `${outcome}|requests=${mock.request_count()}`
    })
}

test "default judgment retries an HTTP 400 until an attempt succeeds" {
    assert.equal(
        retry_over_http([
            root.llm_mock.MockResponse.error(400, rejected_body()),
            root.llm_mock.MockResponse.error(400, rejected_body()),
            root.llm_mock.MockResponse.ok(completed("ok")),
        ]),
        "ok|requests=3",
    )
}

test "default judgment stops at max_attempts and rethrows the HTTP 400" {
    assert.equal(
        retry_over_http([root.llm_mock.MockResponse.error(400, rejected_body())]),
        "InvalidRequest/400|requests=3",
    )
}

function never_retry_invalid_request(f: ai.errors.Failure) -> bool {
    match (f) {
        ai.errors.InvalidRequest => false,
        _ => ai.clients.default_retry_judgment(f),
    }
}

test "a custom retry_if still overrides the default for an HTTP 400" {
    assert.equal(
        retry_over_http(
            [
                root.llm_mock.MockResponse.error(400, rejected_body()),
                root.llm_mock.MockResponse.ok(completed("ok")),
            ],
            retry_if = never_retry_invalid_request,
        ),
        "InvalidRequest/400|requests=1",
    )
}

// Throws an `InvalidRequest` carrying `status_code` on every invocation and
// counts the invocations.
class RejectingClient {
    status_code: int?,
    invocations: int,

    implements ai.Client {
        function requested_model(self) -> string {
            "rejecting"
        }
        function provider(self) -> string {
            "test"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/rejecting",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.invocations += 1;
            throw ai.errors.InvalidRequest {
                provider: "test",
                status_code: self.status_code,
                detail: "rejected",
                raw_body: null,
            };
        }
    }
}

function retry_rejecting(status_code: int?) -> string {
    let inner = RejectingClient {
        status_code: status_code,
        invocations: 0,
    };
    let cl = ai.clients.Retry.new(inner, max_attempts = 3, backoff = short_backoff());
    let outcome = (cl.invoke(input()).terminal_text() ?? "-") catch_all (e) {
        _ => rejection_label(e),
    };
    `${outcome}|invocations=${inner.invocations}`
}

test "default judgment does not retry an InvalidRequest without a status" {
    assert.equal(retry_rejecting(null), "InvalidRequest/-1|invocations=1")
}

test "default judgment retries an InvalidRequest with a status" {
    assert.equal(retry_rejecting(422), "InvalidRequest/422|invocations=3")
}

test "default_retry_judgment on InvalidRequest follows its status" {
    assert.is_true(ai.clients.default_retry_judgment(ai.errors.InvalidRequest {
        provider: "test",
        status_code: 400,
        detail: "http 400",
        raw_body: null,
    }));
    assert.equal(
        false,
        ai.clients.default_retry_judgment(ai.errors.InvalidRequest {
            provider: "client",
            status_code: null,
            detail: "bad config",
            raw_body: null,
        }),
    )
}

test "default_retry_judgment still declines refusals and bad output" {
    assert.equal(
        false,
        ai.clients.default_retry_judgment(ai.errors.Refused {
            provider: "test",
            reason: "declined",
            raw_body: null,
        }),
    );
    assert.equal(
        false,
        ai.clients.default_retry_judgment(ai.errors.ParseFailed {
            provider: "test",
            raw_output: "{}",
            message: null,
        }),
    );
    assert.equal(
        false,
        ai.clients.default_retry_judgment(ai.errors.FinishReasonError {
            provider: "test",
            finish_reason: ai.content.StopReason.MaxTokens,
            raw_output: "{",
        }),
    )
}

// The streaming path judges a 400 answered before any text delta the same way.
function sse_completed(text: string) -> string {
    let events = [
        root.llm_mock.sse_data(
            baml.json.stringify({ "type": "response.output_text.delta", "delta": text }),
        ),
        root.llm_mock.sse_data(
            `{"type":"response.completed","response":{"status":"completed","output":[]}}`,
        ),
    ];
    events.join("\n\n") + "\n\n"
}

function retry_stream_over_http(responses: root.llm_mock.MockResponse[]) -> string {
    root.llm_mock.mock_json_serve(responses, (mock: root.llm_mock.MockJsonServer) -> {
        let cl = ai.clients.Retry.new(
            openai.ResponsesClient.new(api_key = "test", base_url = mock.base_url()),
            max_attempts = 3,
            backoff = short_backoff(),
        );
        let outcome = (
            cl.invoke_stream(input()).final_turn().terminal_text() ?? "-"
        ) catch_all (e) {
            _ => rejection_label(e),
        };
        `${outcome}|requests=${mock.request_count()}`
    })
}

test "streamed Retry reopens after an HTTP 400 before the first delta" {
    assert.equal(
        retry_stream_over_http([
            root.llm_mock.MockResponse.error(400, rejected_body()),
            root.llm_mock.MockResponse.with_headers(200, sse_completed("ok"), {
                "content-type": "text/event-stream",
            }),
        ]),
        "ok|requests=2",
    )
}

test "streamed Retry stops at max_attempts on repeated HTTP 400" {
    assert.equal(
        retry_stream_over_http([root.llm_mock.MockResponse.error(400, rejected_body())]),
        "InvalidRequest/400|requests=3",
    )
}

// Fallback advances on any safely retryable failure without consulting the
// retry judgment, so it moves past a member the provider answered with a 400.
function fallback_past_http_400() -> string {
    root.llm_mock.mock_json_serve(
        [
            root.llm_mock.MockResponse.error(400, rejected_body()),
            root.llm_mock.MockResponse.ok(completed("ok")),
        ],
        (mock: root.llm_mock.MockJsonServer) -> {
            let first = openai.ResponsesClient.new(
                model = "first-model",
                api_key = "test",
                base_url = mock.base_url(),
            );
            let second = openai.ResponsesClient.new(
                model = "second-model",
                api_key = "test",
                base_url = mock.base_url(),
            );
            let members: ai.Client[] = [first, second];
            let turn = ai.clients.Fallback { members: members }.invoke(input());
            let failed = turn.child_turns
                .filter((attempt) -> { attempt.error != null })
                .map((attempt) -> {
                    `${attempt.requested_model ?? "-"}:${rejection_label(attempt.error)}`
                })
                .join(",");
            `${turn.terminal_text() ?? "-"}|${failed}|requests=${mock.request_count()}`
        },
    )
}

test "Fallback advances past a member answered with HTTP 400" {
    assert.equal(
        fallback_past_http_400(),
        "ok|first-model:InvalidRequest/400|requests=2",
    )
}
